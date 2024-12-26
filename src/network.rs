use std::sync::Arc;
use std::time::Duration;
use rand::Rng;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::sync::broadcast::Sender;
use tokio::time::timeout;
use tokio_rustls::rustls::server::WebPkiClientVerifier;
use tokio_rustls::rustls::{ClientConfig, ServerConfig};
use tokio_rustls::{TlsConnector, TlsStream};
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::TlsAcceptor;
use crate::certs::{load_client_cert, load_private_key, load_root_ca};
use crate::{settings, InterTaskMessageToGUI, InterTaskMessageToNetworkTask, LLMMessage, LLMModel, LLMRequest, LLMResponse, LLMResponseBundle, PlayerMessage, TcpMessage};

pub fn spawn_network_task(mpsc_sender: tokio::sync::broadcast::Sender<InterTaskMessageToGUI>, mut restart_receiver: tokio::sync::broadcast::Receiver<()>, restart_receiver2: tokio::sync::broadcast::Receiver<()>, restart_receiver3: tokio::sync::broadcast::Receiver<()>) {
    tokio::spawn(async move {
        let settings: Arc<settings::Settings> = Arc::new(settings::Settings::new().expect("Couldn't read config(s)!"));

        // Load mtls certs
        let root_ca = Arc::new(load_root_ca(settings.root_ca.clone()));
        let client_cert = load_client_cert(settings.client_cert.clone());
        let client_key = load_private_key(settings.client_key.clone());

        // Server Config
        let client_verifier = WebPkiClientVerifier::builder(root_ca.clone()).build().expect("Couldn't build Client Verifier. Check Certs & Key!");

        let server_config = Arc::new(ServerConfig::builder_with_protocol_versions(&[&tokio_rustls::rustls::version::TLS13])
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(client_cert.clone(), client_key.clone_key()).expect("Couldn't build Server Config. Check Certs & Key!"));

        // Client Config
        let client_config = Arc::new(ClientConfig::builder_with_protocol_versions(&[&tokio_rustls::rustls::version::TLS13])
            .with_root_certificates(root_ca)
            .with_client_auth_cert(client_cert, client_key).expect("Couldn't build Client Config. Check Certs & Key!"));


        // Create Server to listen on incoming rendering requests
        let sender_to_gui = Arc::new(mpsc_sender.clone());

        // Create second mpsc channel to receive messages from the GUI
        let (gui_sender, gui_receiver) = tokio::sync::broadcast::channel::<InterTaskMessageToNetworkTask>(30);

        // Send the GUI sender to the GUI task
        sender_to_gui.send(InterTaskMessageToGUI::MspcSender { sender: gui_sender.clone() }).unwrap();

        println!("Started network worker task. Listening for incoming connections...");

        async fn main_worker_task(client_config: Arc<ClientConfig>, listener: TcpListener, acceptor: TlsAcceptor, sender_to_gui: Arc<Sender<InterTaskMessageToGUI>>, mut gui_receiver: broadcast::Receiver<InterTaskMessageToNetworkTask>, restart_receiver2: tokio::sync::broadcast::Receiver<()>, restart_receiver3: tokio::sync::broadcast::Receiver<()>) -> Option<TlsStream<TcpStream>> {
            let res = loop {
                let waiter = listener.accept();
                tokio::select! {
                    Ok((stream, connected_with)) = waiter => {
                        println!("Received connection from {}", connected_with);
                        let acceptor = acceptor.clone();
                        match acceptor.accept(stream).await{
                            Ok(tls_stream1) => {
                                println!("TLS Handshake successful");
                                let tls_stream = TlsStream::from(tls_stream1);
                                sender_to_gui.send(InterTaskMessageToGUI::Connected{ with: connected_with.to_string() }).unwrap();
                                break Some(tls_stream);
                            },
                            Err(e) => {
                                eprintln!("TLS Handshake failed: {}", e);
                            }
                        }
                    }
                    msg = gui_receiver.recv() => {
                            match msg{
                                Ok(msg) => {
                                    match msg{
                                        InterTaskMessageToNetworkTask::StopListening => {
                                            break None;
                                        },

                                    InterTaskMessageToNetworkTask::ConnectTo{ host_string } => {
                                        // Create client connection
                                        let connector = TlsConnector::from(client_config.clone());
                                        match timeout(Duration::from_secs(5), TcpStream::connect(host_string.clone())).await{
                                            Ok(Ok(stream)) => {
                                                match timeout(Duration::from_secs(5), connector.connect(ServerName::try_from("localhost").unwrap(), stream)).await{
                                                    Ok(Ok(tls_stream1)) => {
                                                        println!("Connected!");
                                                        let tls_stream = Some(TlsStream::from(tls_stream1));
                                                        sender_to_gui.send(InterTaskMessageToGUI::Connected{ with: host_string.clone() }).unwrap();
                                                        break tls_stream;
                                                    },
                                                    Ok(Err(e)) => {
                                                        sender_to_gui.send(InterTaskMessageToGUI::ConnectionFailed{error: format!("Couldn't connect to {}: {}", host_string, e)}).unwrap();
                                                        eprintln!("Couldn't connect to {}: {}", host_string, e);
                                                    },
                                                    Err(_) => {
                                                        sender_to_gui.send(InterTaskMessageToGUI::ConnectionFailed{error: format!("Couldn't connect to {}: Timeout", host_string)}).unwrap();
                                                        eprintln!("Couldn't connect to {}: Timeout!", host_string);
                                                    }
                                                }
                                            },
                                            Ok(Err(e)) => {
                                                sender_to_gui.send(InterTaskMessageToGUI::ConnectionFailed{error: format!("Couldn't connect to {}: {}", host_string, e)}).unwrap();
                                                println!("Couldn't connect to {}: {}", host_string, e);
                                            },
                                            Err(_) => {
                                                sender_to_gui.send(InterTaskMessageToGUI::ConnectionFailed{error: format!("Couldn't connect to {}: Timeout", host_string)}).unwrap();
                                                println!("Couldn't connect to {}: Timeout!", host_string);
                                            }
                                        }
                                    },
                                    _ => {
                                        panic!("Unexpected message from GUI: {:?}", msg);
                                    }}
                                }
                                Err(e) => {
                                    eprintln!("Couldn't receive message from GUI: {}", e);
                            }
                            }
                        }
                    }
            };
            println!("loop returned: {:?}", res);
            res
        }

        loop {
            println!("Starting network task");
            let acceptor = TlsAcceptor::from(server_config.clone());
            let listener = TcpListener::bind(format!("{}:{}", settings.bind_to_host, settings.port)).await.unwrap();

            let res = main_worker_task(client_config.clone(), listener, acceptor, sender_to_gui.clone(), gui_sender.subscribe(), restart_receiver2.resubscribe(), restart_receiver3.resubscribe()).await;

            if let Some(tls_stream) = res {
                println!("Handling incoming connection");

                // Create two tasks to handle incoming and outgoing messages
                let (reader, writer) = tokio::io::split(tls_stream);
                handle_writer(writer, gui_receiver.resubscribe(), sender_to_gui.clone(), restart_receiver2.resubscribe());
                handle_reader(reader, sender_to_gui.clone(), restart_receiver3.resubscribe());
            }

            restart_receiver.recv().await.expect("Restart receiver was closed :(");
            println!("Restarting network task");
        }
    });
}

pub fn handle_writer(writer: WriteHalf<TlsStream<TcpStream>>, receiver_from_gui: broadcast::Receiver<InterTaskMessageToNetworkTask>, sender_to_gui: Arc<Sender<InterTaskMessageToGUI>>, mut restart_receiver: tokio::sync::broadcast::Receiver<()>) {
    tokio::spawn(async move {
        async fn loop_write(mut writer: WriteHalf<TlsStream<TcpStream>>, mut receiver_from_gui: broadcast::Receiver<InterTaskMessageToNetworkTask>, sender_to_gui: Arc<Sender<InterTaskMessageToGUI>>) -> Result<(), String> {
            let res = loop {
                let msg_from_gui = receiver_from_gui.recv().await;
                match msg_from_gui {
                    Ok(msg_from_gui) => {
                        match msg_from_gui {
                            InterTaskMessageToNetworkTask::SendMsg { msg } => {
                                println!("Sending message: {:?}", msg);
                                let encoded_msg = match bincode::encode_to_vec(msg, bincode::config::standard()) {
                                    Ok(msg) => msg,
                                    Err(e) => {
                                        eprintln!("Couldn't encode message: {}", e);
                                        break Err(e.to_string());
                                    }
                                };
                                let len = encoded_msg.len() as u64;

                                println!("Sending message of length {}", len);

                                // Send length via socket
                                match timeout(Duration::from_secs(5), writer.write_u64(len)).await {
                                    Ok(Err(e)) => {
                                        eprintln!("Couldn't send message length: {}", e);
                                        break Err(format!("Couldn't send message length: {}", e));
                                    },
                                    Err(_) => {
                                        eprintln!("Couldn't send message length: Timeout");
                                        break Err("Couldn't send message length: Timeout".to_string());
                                    },
                                    _ => {}
                                }

                                match timeout(Duration::from_secs(5), writer.write_all(&encoded_msg[..])).await {
                                    Ok(Err(e)) => {
                                        eprintln!("Couldn't send message: {}", e);
                                        break Err(format!("Couldn't send message: {}", e));
                                    },
                                    Err(_) => {
                                        eprintln!("Couldn't send message: Timeout");
                                        break Err("Couldn't send message: Timeout".to_string());
                                    },
                                    _ => {}
                                }

                                match timeout(Duration::from_secs(5), writer.flush()).await {
                                    Ok(Err(e)) => {
                                        eprintln!("Couldn't flush message: {}", e);
                                        break Err(format!("Couldn't flush message: {}", e));
                                    },
                                    Err(_) => {
                                        eprintln!("Couldn't flush message: Timeout");
                                        break Err("Couldn't flush message: Timeout".to_string());
                                    },
                                    _ => {}
                                }
                            }
                            InterTaskMessageToNetworkTask::ContactLLM { msg, history, client, settings } => {
                                let resp = tokio::time::timeout(Duration::from_secs(30), talk_to_llm(msg, history, client, settings)).await;

                                match resp {
                                    Ok(resp) => {
                                        let sender = sender_to_gui.clone();
                                        tokio::spawn(async move {
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
                                            sender.send(InterTaskMessageToGUI::HandleLLMResponse { response: resp }).expect("Channel to GUI was closed :(");
                                        });
                                    },
                                    Err(_) => {
                                        eprintln!("Couldn't contact LLM, timeout exceeded");
                                    }
                                }
                            }
                            _ => {
                                eprintln!("Received unexpected message from GUI: {:?}", msg_from_gui);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Couldn't receive message from GUI: {}", e);
                        panic!("Channel to GUI was closed :(");
                    }
                }
            };
            println!("loop_write returned: {:?}", res);
            res
        }
        tokio::select! {
            lerror = loop_write(writer, receiver_from_gui, sender_to_gui.clone()) => {
                if let Err(e) = lerror{
                    eprintln!("Error in writing loop: {}", e);
                    sender_to_gui.send(InterTaskMessageToGUI::ConnectionClosedUnexpectedly{error: e}).expect("Channel to GUI was closed :(");

                }
            },
            _ = restart_receiver.recv() => {
                println!("Restarting network task, cancelling writer task");
            },
        }
    });
}

pub fn handle_reader(reader: ReadHalf<TlsStream<TcpStream>>, sender_to_gui: Arc<Sender<InterTaskMessageToGUI>>, mut restart_receiver: tokio::sync::broadcast::Receiver<()>) {
    tokio::spawn(async move {
        println!("Starting to read from socket");

        async fn loop_reading(mut reader: ReadHalf<TlsStream<TcpStream>>, sender_to_gui: Arc<Sender<InterTaskMessageToGUI>>) -> Result<(), String> {
            let res = loop {
                let len = match timeout(Duration::from_secs(300), reader.read_u64()).await {
                    Ok(Ok(len)) => len as usize,
                    Ok(Err(e)) => {
                        eprintln!("Couldn't read message length: {}", e);
                        break Err(format!("Couldn't read message length: {}", e));
                    },
                    Err(_) => {
                        eprintln!("Couldn't read message length: Timeout");
                        break Err("Couldn't read message length: Timeout".to_string());
                    }
                };

                println!("Message length: {}", len);

                let mut buffer = vec![0; len];

                match timeout(Duration::from_secs(300), reader.read_exact(&mut buffer)).await {
                    Ok(Err(e)) => {
                        eprintln!("Couldn't read message: {}", e);
                        break Err(format!("Couldn't read message: {}", e));
                    },
                    Err(_) => {
                        eprintln!("Couldn't read message: Timeout");
                        break Err("Couldn't read message: Timeout".to_string());
                    },
                    _ => {}
                };

                let msg: TcpMessage = match bincode::decode_from_slice(&buffer, bincode::config::standard()) {
                    Ok((msg, _)) => msg,
                    Err(e) => {
                        eprintln!("Couldn't decode message: {}", e);
                        break Err(format!("Couldn't decode message: {}", e));
                    }
                };

                println!("Received message: {:?}", msg);

                sender_to_gui.send(InterTaskMessageToGUI::MessageReceived { msg }).expect("Channel to GUI was closed :(");
            };
            println!("loop_reading returned: {:?}", res);
            res
        }

        tokio::select! {
            lerror = loop_reading(reader, sender_to_gui.clone()) => {
                if let Err(e) = lerror{
                    eprintln!("Error in reading loop: {}", e);
                    sender_to_gui.send(InterTaskMessageToGUI::ConnectionClosedUnexpectedly{error: e}).expect("Channel to GUI was closed :(");

                }
            },
            _ = restart_receiver.recv() => {
                println!("Restarting network task, cancelling reader task");
            },
        }
    });
}

pub async fn talk_to_llm(msg: PlayerMessage, mut history: Vec<LLMMessage>, client: reqwest::Client, settings: Arc<settings::Settings>) -> LLMResponseBundle {
    history.push(LLMMessage {
        role: "user".to_string(),
        content: msg.msg,
        refusal: None,
    });

    let request = LLMRequest {
        model: LLMModel::GPT4o,
        messages: history.clone(),
    };

    println!("Sending request to LLM: {:?}", request);
    let res = client.post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", settings.openai_api_key))
        .json(&request)
        .send().await;

    let mut new_msg = None;

    match res {
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
                }
                Err(e) => {
                    eprintln!("Couldn't decode response from LLM: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Couldn't send request: {}", e);
        }
    }

    LLMResponseBundle {
        new_message_from_llm: new_msg,
        history,
    }
}
