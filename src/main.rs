use std::sync::Arc;
use std::time::SystemTime;
use bincode::{Decode, Encode};
use eframe::egui::{Context, FontData, FontDefinitions, FontFamily, FontId, TextStyle, Ui};
use eframe::{egui, Frame};
use tokio::sync::mpsc;

pub mod certs;
pub mod settings;
pub mod network;
pub mod connect_screen;
pub mod start_screen;
pub mod prompting_screen;

#[derive(Debug, Clone, Default)]
pub enum Screen {
    #[default]
    Connect,
    Start,
    Prompting,
    Game,
    End,
}

#[derive(Debug)]
pub struct ApplicationState {
    pub screen: Screen,
    pub connect_host: String,
    pub name: String,
    pub custom_prompt: String,
    pub warning: Option<String>,
    pub marked_as_ready: bool,
    pub marked_as_ready_opponent: bool,
    pub marked_as_prompt_ready: bool,
    pub marked_as_prompt_ready_opponent: bool,
    pub prompting_start_time: Option<SystemTime>,
    pub settings: settings::Settings,
    pub mpsc_sender: mpsc::Sender<InterTaskMessageToNetworkTask>,
    pub mpsc_receiver: mpsc::Receiver<InterTaskMessageToGUI>
}

impl ApplicationState{
    pub fn new(cc: &eframe::CreationContext<'_>, mpsc_sender: mpsc::Sender<InterTaskMessageToNetworkTask>, mpsc_receiver: mpsc::Receiver<InterTaskMessageToGUI>) -> Self {
        let settings = settings::Settings::new().expect("Failed to load settings");

        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert("pilowlava".to_string(), Arc::new(FontData::from_static(include_bytes!("../fonts/Pilowlava-Regular.otf"))));
        fonts.font_data.insert("spacegrotesk".to_string(), Arc::new(FontData::from_static(include_bytes!("../fonts/SpaceGrotesk-Regular.otf"))));

        fonts.families.get_mut(&FontFamily::Proportional).unwrap()
            .insert(0, "spacegrotesk".to_owned());
        fonts.families.insert(FontFamily::Name("Heading".into()), vec!["pilowlava".to_string()]);

        cc.egui_ctx.set_fonts(fonts);

        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.override_text_color = Some(egui::Color32::from_hex("#FF5053").unwrap());
        style.visuals.window_fill = egui::Color32::from_hex("#0F000A").unwrap();
        style.visuals.panel_fill = egui::Color32::from_hex("#0F000A").unwrap();
        style.visuals.extreme_bg_color = egui::Color32::from_hex("#29114C").unwrap();
        style.text_styles.insert(TextStyle::Heading, FontId::new(20.0, FontFamily::Name("Heading".into())));
        cc.egui_ctx.set_style(style);

        ApplicationState {
            screen: Screen::Connect,
            connect_host: "".to_string(),
            name: "".to_string(),
            custom_prompt: "".to_string(),
            warning: None,
            marked_as_ready: false,
            marked_as_ready_opponent: false,
            marked_as_prompt_ready: false,
            marked_as_prompt_ready_opponent: false,
            prompting_start_time: None,
            settings,
            mpsc_sender,
            mpsc_receiver,
        }
    }
}

impl eframe::App for ApplicationState{
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        if !self.mpsc_receiver.is_empty(){
            match self.mpsc_receiver.try_recv(){
                Ok(msg) => {
                    println!("Received message: {:?}", msg);
                    match msg {
                        InterTaskMessageToGUI::Connected { with } => {
                            self.screen = Screen::Start;
                        },
                        InterTaskMessageToGUI::ConnectionFailed { error } => {
                            self.warning = Some(error);
                        },
                        InterTaskMessageToGUI::ConnectionClosed => {
                            self.screen = Screen::Connect;
                            self.custom_prompt = "".to_string();
                        },
                        InterTaskMessageToGUI::MessageReceived { msg } => {
                            match msg {
                                TcpMessage::MarkedAsReady => {
                                    self.marked_as_ready_opponent = true;
                                },
                                TcpMessage::PromptingFinished => {
                                    self.marked_as_prompt_ready_opponent = true;
                                },
                                TcpMessage::EndGame => {
                                    self.screen = Screen::End;
                                },
                                TcpMessage::Message(player_message) => {
                                    println!("Received message: {:?}", player_message); //TODO
                                }
                            }
                        }
                        InterTaskMessageToGUI::ListenForConnections => {}
                        InterTaskMessageToGUI::MspcSender { .. } => {}
                    }
                },
                Err(e) => {
                    eprintln!("Error receiving message: {}", e);
                }
            }
        }

        if let Screen::Start = self.screen{
            if self.marked_as_ready && self.marked_as_ready_opponent{
                self.screen = Screen::Prompting;
                self.prompting_start_time = Some(SystemTime::now())
            }
        }
        if let Screen::Prompting = self.screen{
            if self.prompting_start_time.unwrap().elapsed().unwrap().as_secs() >= 90{
                self.marked_as_prompt_ready = true;
            }
            if self.marked_as_prompt_ready && self.marked_as_prompt_ready_opponent{
                self.screen = Screen::Game;
            }
        }

        match self.screen{
            Screen::Connect => {
                connect_screen::render_connect_screen(self, ctx, frame);
            }
            Screen::Start => {
                start_screen::render_start_screen(self, ctx, frame);
            },
            Screen::Prompting => {
                prompting_screen::render_prompting_screen(self, ctx, frame);
            }
            Screen::Game => {}
            Screen::End => {}
        }
    }
}

#[derive(Debug, Clone, Default)]
pub enum InterTaskMessageToGUI {
    #[default]
    ListenForConnections,
    MspcSender{
        sender: mpsc::Sender<InterTaskMessageToNetworkTask>,
    },
    Connected{
        with: String,
    },
    MessageReceived{
        msg: TcpMessage,
    },
    ConnectionFailed{
        error: String,
    },
    ConnectionClosed
}

#[derive(Debug, Clone, Default)]
pub enum InterTaskMessageToNetworkTask {
    #[default]
    StopListening,
    ConnectTo{
        host_string: String,
    },
    SendMsg{
        msg: TcpMessage,
    },
    //TODO: add ConnectionClosed, keepalive, etc.
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct PlayerMessage{
    pub msg: String,
    pub chat: u8,
    pub timestamp: u64,
}

#[derive(Clone, Debug, Encode, Decode)]
pub enum TcpMessage{
    MarkedAsReady,
    PromptingFinished,
    Message(PlayerMessage),
    EndGame,
}

#[tokio::main]
pub async fn main()  {
    let options = eframe::NativeOptions::default();

    let (sender_to_gui, mut receiver_from_network) = mpsc::channel::<InterTaskMessageToGUI>(100);

    // Start the network task
    network::spawn_network_task(sender_to_gui.clone());

    // Get the sender to the network task
    let msg = receiver_from_network.recv().await.unwrap();
    let sender_to_network ;

    if let InterTaskMessageToGUI::MspcSender {
        sender
    } = msg {
        sender_to_network = sender;
    }else{
        panic!("Expected MspcSender message from network task");
    };

    eframe::run_native(
        "The Turing Challenge",
        options,
        Box::new(|cc| Ok(Box::new({
            ApplicationState::new(cc, sender_to_network, receiver_from_network)
        })),),
    ).expect("Couldn't start GUI");
}
