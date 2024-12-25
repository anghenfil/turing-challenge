use std::sync::Arc;
use rand::Rng;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, MutexGuard};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_rustls::rustls::server::WebPkiClientVerifier;
use tokio_rustls::rustls::{ClientConfig, ClientConnection, ServerConfig};
use tokio_rustls::{TlsConnector, TlsStream};
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::TlsAcceptor;
use crate::certs::{load_client_cert, load_private_key, load_root_ca};
use crate::{settings, InterTaskMessageToGUI, InterTaskMessageToNetworkTask, LLMMessage, LLMModel, LLMRequest, LLMResponse, LLMResponseBundle, PlayerMessage, TcpMessage};

pub fn spawn_network_task(mpsc_sender: tokio::sync::mpsc::Sender<InterTaskMessageToGUI>){
    tokio::spawn(async move {
        let settings : Arc<settings::Settings> = Arc::new(settings::Settings::new().expect("Couldn't read config(s)!"));

        // Load mtls certs
        let root_ca = Arc::new(load_root_ca(settings.root_ca.clone()));
        let client_cert = load_client_cert(settings.client_cert.clone());
        let client_key = load_private_key(settings.client_key.clone());

        // Server Config
        let client_verifier = WebPkiClientVerifier::builder(root_ca.clone()).build().expect("Couldn't build Client Verifier. Check Certs & Key!");

        let server_config = ServerConfig::builder_with_protocol_versions(&[&tokio_rustls::rustls::version::TLS13])
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(client_cert.clone(), client_key.clone_key()).expect("Couldn't build Server Config. Check Certs & Key!");

        // Client Config
        let client_config = Arc::new(ClientConfig::builder_with_protocol_versions(&[&tokio_rustls::rustls::version::TLS13])
            .with_root_certificates(root_ca)
            .with_client_auth_cert(client_cert, client_key).expect("Couldn't build Client Config. Check Certs & Key!"));


        // Create Server to listen on incoming rendering requests
        let acceptor = TlsAcceptor::from(Arc::new(server_config));
        let listener = TcpListener::bind(format!("{}:{}", settings.bind_to_host, settings.port)).await.unwrap();
        let sender_to_gui = Arc::new(mpsc_sender.clone());

        // Create second mpsc channel to receive messages from the GUI
        let (gui_sender, mut gui_receiver) = tokio::sync::mpsc::channel::<InterTaskMessageToNetworkTask>(30);

        // Send the GUI sender to the GUI task
        sender_to_gui.send(InterTaskMessageToGUI::MspcSender { sender: gui_sender }).await.unwrap();

        println!("Started network worker task. Listening for incoming connections...");

        let mut tls_stream: Option<TlsStream<TcpStream>> = None;

        loop {
            let waiter = listener.accept();
            tokio::select! {
                Ok((stream, connected_with)) = waiter => {
                    println!("Received connection from {}", connected_with);
                    let acceptor = acceptor.clone();
                    match acceptor.accept(stream).await{
                        Ok(tls_stream1) => {
                            println!("TLS Handshake successful");
                            tls_stream = Some(TlsStream::from(tls_stream1));
                            sender_to_gui.send(InterTaskMessageToGUI::Connected{ with: connected_with.to_string() }).await.unwrap();
                            break;
                        },
                        Err(e) => {
                            eprintln!("TLS Handshake failed: {}", e);
                        }
                    }
                }
                msg = gui_receiver.recv() => {
                        match msg{
                            Some(msg) => {
                                match msg{
                                    InterTaskMessageToNetworkTask::StopListening => {
                                        break;
                                    },

                                InterTaskMessageToNetworkTask::ConnectTo{ host_string } => {
                                    // Create client connection
                                    let connector = TlsConnector::from(client_config.clone());
                                    match TcpStream::connect(host_string.clone()).await{
                                        Ok(stream) => {
                                            match connector.connect(ServerName::try_from("localhost").unwrap(), stream).await{
                                                Ok(tls_stream1) => {
                                                    println!("Connected!");
                                                    tls_stream = Some(TlsStream::from(tls_stream1));
                                                    sender_to_gui.send(InterTaskMessageToGUI::Connected{ with: host_string.clone() }).await.unwrap();
                                                    break;
                                                },
                                                Err(e) => {
                                                    sender_to_gui.send(InterTaskMessageToGUI::ConnectionFailed{error: format!("Couldn't connect to {}: {}", host_string, e)}).await.unwrap();
                                                    eprintln!("Couldn't connect to {}: {}", host_string, e);
                                                }
                                            }
                                        },
                                        Err(e) => {
                                            sender_to_gui.send(InterTaskMessageToGUI::ConnectionFailed{error: format!("Couldn't connect to {}: {}", host_string, e)}).await.unwrap();
                                            println!("Couldn't connect to {}: {}", host_string, e);
                                        }
                                    }
                                },
                                _ => {
                                    panic!("Unexpected message from GUI: {:?}", msg);
                                }}
                            }
                            None => {}
                        }
                    }
                }
            }

        if let Some(tls_stream) = tls_stream{
            println!("Handling incoming connection");

            // Create two tasks to handle incoming and outgoing messages
            let (mut reader, mut writer) = tokio::io::split(tls_stream);
            handle_writer(writer, gui_receiver, sender_to_gui.clone());
            handle_reader(reader, sender_to_gui.clone())
        }
    });
}

pub fn handle_writer(mut writer: WriteHalf<TlsStream<TcpStream>>, mut receiver_from_gui: Receiver<InterTaskMessageToNetworkTask>, sender_to_gui: Arc<Sender<InterTaskMessageToGUI>>){
    tokio::spawn(async move{
        loop{
            let msg_from_gui = receiver_from_gui.recv().await;
            match msg_from_gui{
                Some(msg_from_gui) => {
                    match msg_from_gui{
                        InterTaskMessageToNetworkTask::SendMsg { msg } => {
                            println!("Sending message: {:?}", msg);
                            let encoded_msg = match bincode::encode_to_vec(msg, bincode::config::standard()){
                                Ok(msg) => msg,
                                Err(e) => {
                                    panic!("Couldn't encode message: {}", e);
                                }
                            };
                            let len = encoded_msg.len() as u64;

                            println!("Sending message of length {}", len);

                            // Send length via socket
                            if let Err(e) = writer.write_u64(len).await{
                                eprintln!("Couldn't send message length: {}", e); //TODO: Handle error, try again, reconnect, etc.
                                break;
                            }

                            if let Err(e) = writer.write_all(&encoded_msg[..]).await{
                                eprintln!("Couldn't send message: {}", e); //TODO: Handle error, try again, reconnect, etc.
                                break;
                            }

                            if let Err(e) = writer.flush().await{
                                eprintln!("Couldn't flush message: {}", e); //TODO: Handle error, try again, reconnect, etc.
                                break;
                            }

                        },
                        InterTaskMessageToNetworkTask::ContactLLM {msg, history, client, settings} => {
                            let resp = talk_to_llm(msg, history, client, settings).await;

                            let sender = sender_to_gui.clone();
                            tokio::spawn(async move{
                                if let Some(msg) = &resp.new_message_from_llm {
                                    // calculate random delay before sending responsedelay

                                    let num_of_chars = msg.chars().count();

                                    let chars_per_second: f32;
                                    {
                                        let mut rng = rand::thread_rng();
                                        chars_per_second = rng.gen_range(2.0..3.5);
                                    }

                                    let delay = num_of_chars as f32 / chars_per_second;
                                    let delay_in_ms = (delay * 100.0) as u64;
                                    println!("Delaying response by {} ms aka {} chars per second", delay_in_ms, chars_per_second);
                                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_in_ms)).await;
                                }
                                sender.send(InterTaskMessageToGUI::HandleLLMResponse { response: resp }).await.expect("Channel to GUI was closed :(");
                            });
                        }
                        _ => {
                            eprintln!("Received unexpected message from GUI: {:?}", msg_from_gui);
                        }
                    }
                },
                None => {
                    panic!("Channel to GUI was closed :(");
                }
            }
        }
    });
}

pub fn handle_reader(mut reader: ReadHalf<TlsStream<TcpStream>>, sender_to_gui: Arc<Sender<InterTaskMessageToGUI>>){
    tokio::spawn(async move{
        println!("Starting to read from socket");

        loop{
            let len = match reader.read_u64().await{
                Ok(len) => len as usize,
                Err(e) => {
                    eprintln!("Couldn't read message length: {}", e); //TODO: Handle error, try again, reconnect, etc.
                    break;
                }
            };

            println!("Message length: {}", len);

            let mut buffer = vec![0; len];

            if let Err(e) = reader.read_exact(&mut buffer).await{
                eprintln!("Couldn't read message: {}", e); //TODO: Handle error, try again, reconnect, etc.
                break;
            };

            let msg: TcpMessage = match bincode::decode_from_slice(&buffer, bincode::config::standard()){
                Ok((msg, _)) => msg,
                Err(e) => {
                    eprintln!("Couldn't decode message: {}", e); //TODO: Handle error, try again, reconnect, etc.
                    break;
                }
            };

            println!("Received message: {:?}", msg);

            sender_to_gui.send(InterTaskMessageToGUI::MessageReceived { msg }).await.expect("Channel to GUI was closed :(");

        }
    });
}

pub async fn talk_to_llm(msg: PlayerMessage, mut history: Vec<LLMMessage>, client: reqwest::Client, settings: Arc<settings::Settings>) -> LLMResponseBundle{
    history.push(LLMMessage{
        role: "user".to_string(),
        content: msg.msg,
        refusal: None,
    });

    let request = LLMRequest{
        model: LLMModel::GPT4o,
        messages: history.clone(),
    };

    println!("Sending request to LLM: {:?}", request);
    let res = client.post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", settings.openai_api_key))
        .json(&request)
        .send().await;

    let mut new_msg = None;

    match res{
        Ok(res) => {
                let res = res.json::<LLMResponse>().await;
                match res {
                    Ok(res) => {
                        println!("Received response from LLM: {:?}", res);
                        if let Some(res) = res.choices.first() {
                            if res.finish_reason != String::from("stop") {
                                eprintln!("LLM didn't finish conversation. This is unexpected!");
                            }
                            new_msg = Some(res.message.content.clone());
                            history.push(res.message.clone())
                        } else {
                            eprintln!("LLM didn't return any choices. This is unexpected!");
                        }
                    },
                    Err(e) => {
                        eprintln!("Couldn't decode response from LLM: {}", e);
                    }
                }

        },
        Err(e) => {
            eprintln!("Couldn't send request: {}", e);
        }
    }

    LLMResponseBundle{
        new_message_from_llm: new_msg,
        history,
    }
}
