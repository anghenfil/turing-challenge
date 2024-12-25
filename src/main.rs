use std::fmt::Display;
use std::sync::Arc;
use std::time::SystemTime;
use bincode::{Decode, Encode};
use eframe::egui::{Context, FontData, FontDefinitions, FontFamily, FontId, TextStyle, Ui};
use eframe::{egui, Frame};
use eframe::emath::History;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize, Serializer};
use serde::ser::SerializeMap;
use tokio::sync::mpsc;
use crate::settings::Settings;

pub mod certs;
pub mod settings;
pub mod network;
pub mod connect_screen;
pub mod start_screen;
pub mod prompting_screen;
pub mod game_screen;
pub mod end_screen;

#[derive(Debug, Clone, Default)]
pub enum Screen {
    #[default]
    Connect,
    Start,
    Prompting,
    Game,
    End,
    End2,
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
    pub chat1_input: String,
    pub chat2_input: String,
    pub chat1_history: Vec<ChatMessage>,
    pub chat2_history: Vec<ChatMessage>,
    // Which foreign chat belongs to the real human?
    pub human_chat: u8,
    pub prompting_start_time: Option<SystemTime>,
    pub game_start_time: Option<SystemTime>,
    pub llm_history: Vec<LLMMessage>,
    pub llm_take_iniative_after: u8,
    pub llm_chat_first_message: bool,
    pub correctly_guessed: Option<bool>,
    pub showing_end_screen_since: Option<SystemTime>,
    pub reqwest_client: Client,
    pub settings: Arc<settings::Settings>,
    pub mpsc_sender: mpsc::Sender<InterTaskMessageToNetworkTask>,
    pub mpsc_receiver: mpsc::Receiver<InterTaskMessageToGUI>
}

#[derive(Debug, Clone)]
struct LLMResponseBundle{
    new_message_from_llm: Option<String>,
    history: Vec<LLMMessage>,
}

#[derive(Debug, Deserialize)]
struct LLMRequest{
    model: LLMModel,
    messages: Vec<LLMMessage>,

}

impl Serialize for LLMRequest{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("model", &self.model.to_string())?;
        map.serialize_entry("messages", &self.messages)?;
        map.end()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct LLMResponse{
    id: String,
    choices: Vec<LLMResponseChoice>,
    created: u64,
    model: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct LLMResponseChoice{
    finish_reason: String,
    index: u8,
    message: LLMMessage,
}

#[derive(Debug, Serialize, Deserialize)]
enum LLMModel{
    #[serde(rename = "chatgpt-4o-latest")]
    GPT4o,
    #[serde(rename = "o1")]
    GPTo1,
}

impl Display for LLMModel{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            LLMModel::GPT4o => "chatgpt-4o-latest".to_string(),
            LLMModel::GPTo1 => "o1".to_string(),
        };
        write!(f, "{}", str)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct LLMMessage{
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    refusal: Option<String>
}


#[derive(Debug, Serialize, Deserialize, Clone)]
enum LLMMessageRole{
    #[serde(rename = "developer")]
    Developer,
    #[serde(rename = "user")]
    User
}

impl Display for LLMMessageRole{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            LLMMessageRole::Developer => "developer".to_string(),
            LLMMessageRole::User => "user".to_string(),
        };
        write!(f, "{}", str)
    }
}

#[derive(Debug)]
struct ChatMessage{
    timestamp: SystemTime,
    message: String,
    from: ChatMessageOrigin
}

#[derive(Debug)]
enum ChatMessageOrigin{
    Own,
    Foreign
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

        // Generate randomly which foreign chat belongs to the real human
        let mut rng = rand::thread_rng();
        let human_chat : u8= rng.gen_range(0..=1);
        let llm_take_iniative_after = rng.gen_range(0..=15);

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
            chat1_input: "".to_string(),
            chat2_input: "".to_string(),
            chat1_history: vec![],
            chat2_history: vec![],
            human_chat,
            prompting_start_time: None,
            game_start_time: None,
            llm_history: vec![],
            llm_take_iniative_after,
            llm_chat_first_message: false,
            correctly_guessed: None,
            showing_end_screen_since: None,
            reqwest_client: Client::new(),
            settings: Arc::new(settings),
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
                                    if player_message.to_ai{
                                        self.llm_chat_first_message = true;
                                        self.mpsc_sender.try_send(InterTaskMessageToNetworkTask::ContactLLM {
                                            msg: player_message,
                                            history: self.llm_history.clone(),
                                            client: self.reqwest_client.clone(),
                                            settings: self.settings.clone()
                                        }).unwrap();
                                    }else{
                                        if player_message.from_ai{
                                            if self.human_chat == 0{
                                                self.chat2_history.push(ChatMessage{
                                                    timestamp: player_message.timestamp,
                                                    message: player_message.msg,
                                                    from: ChatMessageOrigin::Foreign
                                                });
                                            }else{
                                                self.chat1_history.push(ChatMessage{
                                                    timestamp: player_message.timestamp,
                                                    message: player_message.msg,
                                                    from: ChatMessageOrigin::Foreign
                                                });
                                            }
                                        }else{
                                            if self.human_chat == 0{
                                                self.chat1_history.push(ChatMessage{
                                                    timestamp: player_message.timestamp,
                                                    message: player_message.msg,
                                                    from: ChatMessageOrigin::Foreign
                                                });
                                            }else{
                                                self.chat2_history.push(ChatMessage{
                                                    timestamp: player_message.timestamp,
                                                    message: player_message.msg,
                                                    from: ChatMessageOrigin::Foreign
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        InterTaskMessageToGUI::ListenForConnections => {}
                        InterTaskMessageToGUI::MspcSender { .. } => {}
                        InterTaskMessageToGUI::HandleLLMResponse { response } => {
                            self.llm_history = response.history;

                            if let Some(new_msg) = response.new_message_from_llm {
                                // Send the message to the opponent
                                self.mpsc_sender.try_send(InterTaskMessageToNetworkTask::SendMsg {
                                    msg: TcpMessage::Message(PlayerMessage {
                                        msg: new_msg.clone(),
                                        from_ai: true,
                                        timestamp: SystemTime::now(),
                                        to_ai: false,
                                    })
                                }).unwrap();
                            }
                        }
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
                // Create first messages for LLM
                self.llm_history.push(LLMMessage{
                    role: "developer".to_string(),
                    content: self.settings.initial_prompt.clone(),
                    refusal: None,
                });
                self.llm_history.push(LLMMessage{
                    role: "developer".to_string(),
                    content: self.custom_prompt.clone(),
                    refusal: None,
                });

                self.screen = Screen::Game;
                self.game_start_time = Some(SystemTime::now());
            }
        }

        if let Screen::Game = self.screen{
            if self.game_start_time.unwrap().elapsed().unwrap().as_secs() >= 210{
                self.screen = Screen::End;
            }
        }

        if let Screen::End2 = self.screen{
            if self.showing_end_screen_since.unwrap().elapsed().unwrap().as_secs() >= 10{
                self.screen = Screen::Connect;
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
            Screen::Game => {
                game_screen::render_game_screen(self, ctx, frame);
            }
            Screen::End => {
                end_screen::render_end_screen(self, ctx, frame);
            },
            Screen::End2 => {
                end_screen::render_end_screen2(self, ctx, frame);
            }
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
    ConnectionClosed,
    HandleLLMResponse{
        response: LLMResponseBundle,
    }
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
    ContactLLM{
        msg: PlayerMessage,
        history: Vec<LLMMessage>,
        client: Client,
        settings: Arc<Settings>,
    },
    //TODO: add ConnectionClosed, keepalive, etc.
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct PlayerMessage{
    pub msg: String,
    pub from_ai: bool,
    pub to_ai: bool,
    pub timestamp: SystemTime,
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
