use std::sync::Arc;
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
use crate::{settings, InterTaskMessageToGUI, InterTaskMessageToNetworkTask, TcpMessage};

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
                                InterTaskMessageToNetworkTask::SendMsg{ .. } => {}}
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
            handle_writer(writer, gui_receiver);
            handle_reader(reader, sender_to_gui.clone())
        }
    });
}

pub fn handle_writer(mut writer: WriteHalf<TlsStream<TcpStream>>, mut receiver_from_gui: Receiver<InterTaskMessageToNetworkTask>){
    tokio::spawn(async move{
        loop{
            let msg_from_gui = receiver_from_gui.recv().await;
            match msg_from_gui{
                Some(msg_from_gui) => {
                    match msg_from_gui{
                        InterTaskMessageToNetworkTask::SendMsg { msg } => {
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

pub async fn send_to_network_task(sender: Arc<Option<Mutex<Sender<InterTaskMessageToNetworkTask>>>>, message: InterTaskMessageToNetworkTask){
    if let Some(sender) = &*sender {
        let mut sender = sender.lock().await;
        sender.send(message).await.unwrap();
    } else {
        panic!("Sender not initialized!");
    }
}