use std::fmt::Display;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use bincode::{Decode, Encode};
use eframe::egui::{Context, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use eframe::{egui, Frame};
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize, Serializer};
use serde::ser::SerializeMap;
use tokio::sync::broadcast;
use crate::settings::Settings;

pub mod certs;
pub mod settings;
pub mod network;
pub mod start_screen;
pub mod welcome_screen;
pub mod prompting_screen;
pub mod game_screen;
pub mod end_screen;

#[derive(Debug, Clone, Default)]
pub enum Screen {
    #[default]
    Start,
    Welcome,
    Prompting,
    Game,
    End,
    End2,
}

#[derive(Debug)]
pub struct ApplicationState {
    pub start_game_pressed: bool,
    pub screen: Screen,
    pub name: String,
    pub custom_prompt: String,
    pub warning: Option<String>,
    pub marked_as_ready: bool,
    pub marked_as_ready_opponent: bool,
    pub waiting_for_ready_opponent_since: Option<SystemTime>,
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
    pub llm_last_message_time: Option<SystemTime>,
    pub llm_noresponse_iniative_time: u8,
    pub correctly_guessed: Option<bool>,
    pub showing_end_screen_since: Option<SystemTime>,
    pub reqwest_client: Client,
    pub settings: Arc<settings::Settings>,
    pub mpsc_sender: tokio::sync::broadcast::Sender<InterTaskMessageToNetworkTask>,
    pub mpsc_receiver: tokio::sync::broadcast::Receiver<InterTaskMessageToGUI>,
    pub mpsc_restart_sender: tokio::sync::broadcast::Sender<()>,
    pub last_message_time_own: Option<SystemTime>,
    pub last_message_time_foreign: Option<SystemTime>,
    pub chars_per_second_lower: f32,
    pub chars_per_second_upper: f32,
    /// Previous human response times in chars per second
    pub human_response_times_chars_per_second: Vec<f32>,
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
    pub fn new(cc: &eframe::CreationContext<'_>, mpsc_sender: broadcast::Sender<InterTaskMessageToNetworkTask>, mpsc_receiver: broadcast::Receiver<InterTaskMessageToGUI>, mpsc_restart_sender: broadcast::Sender<()>) -> Self {
        let settings = settings::Settings::new().expect("Failed to load settings");

        let previous_human_response_times = load_previous_human_response_times();

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
        style.text_styles.insert(TextStyle::Heading, FontId::new(30.0, FontFamily::Name("Heading".into())));
        style.text_styles.insert(TextStyle::Body, FontId::new(20.0, FontFamily::Proportional));
        cc.egui_ctx.set_style(style);

        // Generate randomly which foreign chat belongs to the real human
        let mut rng = rand::thread_rng();
        let human_chat : u8= rng.gen_range(0..=1);
        let llm_take_iniative_after = rng.gen_range(0..=15);
        let llm_noresponse_iniative_time = rng.gen_range(10..=20);

        let (chars_per_second_lower, chars_per_second_upper) = calculate_average_chars_per_second_limits(&previous_human_response_times);

        ApplicationState {
            start_game_pressed: false,
            screen: Screen::Start,
            name: "".to_string(),
            custom_prompt: "".to_string(),
            warning: None,
            marked_as_ready: false,
            marked_as_ready_opponent: false,
            waiting_for_ready_opponent_since: None,
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
            llm_last_message_time: None,
            llm_noresponse_iniative_time,
            correctly_guessed: None,
            showing_end_screen_since: None,
            reqwest_client: Client::new(),
            settings: Arc::new(settings),
            mpsc_sender,
            mpsc_receiver,
            mpsc_restart_sender,
            human_response_times_chars_per_second: vec![],
            chars_per_second_upper,
            chars_per_second_lower,
            last_message_time_own: None,
            last_message_time_foreign: None,
        }
    }
}

impl eframe::App for ApplicationState{
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        ctx.request_repaint_after(Duration::from_secs(1));
        if !self.mpsc_receiver.is_empty(){
            match self.mpsc_receiver.try_recv(){
                Ok(msg) => {
                    println!("Received message: {:?}", msg);
                    match msg {
                        InterTaskMessageToGUI::Connected { with } => {
                            self.screen = Screen::Welcome;
                        },
                        InterTaskMessageToGUI::ConnectionFailed { error } => {
                            self.warning = Some(error);
                            self.start_game_pressed = false;
                        },
                        InterTaskMessageToGUI::ConnectionClosedUnexpectedly {error} => {
                            println!("Connection closed unexpectedly: {}", error);
                            // Save the human response times
                            save_human_response_times_to_file(&self.human_response_times_chars_per_second);
                            // Calculate the new average chars per second
                            let (lower, upper) = calculate_average_chars_per_second_limits(&self.human_response_times_chars_per_second);
                            self.chars_per_second_lower = lower;
                            self.chars_per_second_upper = upper;

                            // Restart
                            // Save the human response times
                            save_human_response_times_to_file(&self.human_response_times_chars_per_second);
                            // Calculate the new average chars per second
                            let (lower, upper) = calculate_average_chars_per_second_limits(&self.human_response_times_chars_per_second);
                            self.chars_per_second_lower = lower;
                            self.chars_per_second_upper = upper;

                            reset_app_state(self);
                            self.mpsc_restart_sender.send(()).unwrap();
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
                                        self.llm_last_message_time = Some(SystemTime::now());
                                        self.llm_chat_first_message = true;
                                        self.mpsc_sender.send(InterTaskMessageToNetworkTask::ContactLLM {
                                            msg: player_message,
                                            history: self.llm_history.clone(),
                                            client: self.reqwest_client.clone(),
                                            settings: self.settings.clone(),
                                            lower_delay_limit: self.chars_per_second_lower,
                                            upper_delay_limit: self.chars_per_second_upper,
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
                                            // Message not from AI, calculate response time
                                            match self.last_message_time_foreign{
                                                Some(time) => {
                                                    let elapsed = time.elapsed().unwrap();
                                                    let elapsed_secs = elapsed.as_millis();

                                                    let chars_per_second = player_message.msg.len() as f32 / elapsed_secs as f32;
                                                    println!("Added foreign chars per second: {}", chars_per_second);
                                                    self.human_response_times_chars_per_second.push(chars_per_second);

                                                    self.last_message_time_foreign = Some(SystemTime::now());
                                                },
                                                None => {
                                                    self.last_message_time_foreign = Some(SystemTime::now());
                                                }
                                            }
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
                                self.mpsc_sender.send(InterTaskMessageToNetworkTask::SendMsg {
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

        if let Screen::Welcome = self.screen{
            if self.marked_as_ready && !self.marked_as_ready_opponent{
                match self.waiting_for_ready_opponent_since{
                    Some(time) => {
                        if time.elapsed().unwrap().as_secs() >= 30{
                            // Save the human response times
                            save_human_response_times_to_file(&self.human_response_times_chars_per_second);
                            // Calculate the new average chars per second
                            let (lower, upper) = calculate_average_chars_per_second_limits(&self.human_response_times_chars_per_second);
                            self.chars_per_second_lower = lower;
                            self.chars_per_second_upper = upper;

                            reset_app_state(self);
                            self.mpsc_restart_sender.send(()).unwrap();
                            self.screen = Screen::Start;
                        }
                    },
                    None => {
                        self.waiting_for_ready_opponent_since = Some(SystemTime::now());
                    }
                }
            }

            if self.marked_as_ready && self.marked_as_ready_opponent{
                self.screen = Screen::Prompting;
                self.prompting_start_time = Some(SystemTime::now())
            }
        }
        if let Screen::Prompting = self.screen{
            if self.prompting_start_time.unwrap().elapsed().unwrap().as_secs() >= 90{
                self.marked_as_prompt_ready = true;
                self.mpsc_sender.send(InterTaskMessageToNetworkTask::SendMsg {
                    msg: TcpMessage::PromptingFinished
                }).unwrap();
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
            if self.prompting_start_time.unwrap().elapsed().unwrap().as_secs() >= 120{
                // Prompting time is over since 30 seconds but the opponent hasn't marked as ready -> reset
                // Save the human response times
                save_human_response_times_to_file(&self.human_response_times_chars_per_second);
                // Calculate the new average chars per second
                let (lower, upper) = calculate_average_chars_per_second_limits(&self.human_response_times_chars_per_second);
                self.chars_per_second_lower = lower;
                self.chars_per_second_upper = upper;


                reset_app_state(self);
                self.mpsc_restart_sender.send(()).unwrap();
                self.screen = Screen::Start;
            }
        }

        if let Screen::Game = self.screen{
            if self.game_start_time.unwrap().elapsed().unwrap().as_secs() >= 210{
                self.screen = Screen::End;
            }
        }

        if let Screen::End2 = self.screen{
            if self.showing_end_screen_since.unwrap().elapsed().unwrap().as_secs() >= 10{
                // Save the human response times
                save_human_response_times_to_file(&self.human_response_times_chars_per_second);
                // Calculate the new average chars per second
                let (lower, upper) = calculate_average_chars_per_second_limits(&self.human_response_times_chars_per_second);
                self.chars_per_second_lower = lower;
                self.chars_per_second_upper = upper;

                // Restart the app
                reset_app_state(self);
                self.mpsc_restart_sender.send(()).unwrap();
                self.screen = Screen::Start;
            }
        }

        match self.screen{
            Screen::Start => {
                start_screen::render_start_screen(self, ctx, frame);
            }
            Screen::Welcome => {
                welcome_screen::render_welcome_screen(self, ctx, frame);
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
        sender: broadcast::Sender<InterTaskMessageToNetworkTask>,
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
    ConnectionClosedUnexpectedly{
        error: String,
    },
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
        lower_delay_limit: f32,
        upper_delay_limit: f32,
    },
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

    let (sender_to_gui, mut receiver_from_network) = broadcast::channel::<InterTaskMessageToGUI>(100);

    let (restart_sender, restart_receiver) = tokio::sync::broadcast::channel::<()>(1);

    // Start the network task
    network::spawn_network_task(sender_to_gui.clone(), restart_sender.subscribe(), restart_sender.subscribe(), restart_sender.subscribe());

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
            ApplicationState::new(cc, sender_to_network, receiver_from_network, restart_sender.clone())
        })),),
    ).expect("Couldn't start GUI");
}

/// Calculate the average chars per second from a list of response times
/// Returns the lower and upper limits for random generation
fn calculate_average_chars_per_second_limits(response_times: &Vec<f32>) -> (f32, f32){
    let sum: f32 = response_times.iter().sum();
    let len = response_times.len();
    if len == 0{
        return (2.0, 3.5); // Default values if we dont have any values yet
    }
    let avg = sum as f32 / len as f32;

    // Calculate the standard deviation
    let variance = response_times.iter().map(|v| {
        let diff = *v - avg;
        diff * diff
    }).sum::<f32>() / len as f32;

    let std_dev = variance.sqrt();

    // Convert vom ms to seconds:
    let avg = avg * 1000.0;
    let std_dev = std_dev * 1000.0;

    println!("Average: {}, Std Dev: {}", avg, std_dev);
    (avg - std_dev, avg + std_dev)
}

fn reset_app_state(state: &mut ApplicationState){
    let mut rng = rand::thread_rng();
    let human_chat : u8= rng.gen_range(0..=1);
    let llm_take_iniative_after = rng.gen_range(12..=25);
    let (lower_limit, upper_limit) = calculate_average_chars_per_second_limits(&state.human_response_times_chars_per_second);

    state.start_game_pressed = false;
    state.screen = Screen::Start;
    state.name = "".to_string();
    state.warning = None;
    state.custom_prompt = "".to_string();
    state.marked_as_ready = false;
    state.marked_as_ready_opponent = false;
    state.marked_as_prompt_ready = false;
    state.marked_as_prompt_ready_opponent = false;
    state.chat1_input = "".to_string();
    state.chat2_input = "".to_string();
    state.chat1_history = vec![];
    state.chat2_history = vec![];
    state.human_chat = human_chat;
    state.prompting_start_time = None;
    state.game_start_time = None;
    state.llm_history = vec![];
    state.llm_take_iniative_after = llm_take_iniative_after;
    state.llm_chat_first_message = false;
    state.correctly_guessed = None;
    state.showing_end_screen_since = None;
    state.waiting_for_ready_opponent_since = None;
    state.llm_last_message_time = None;
    state.chars_per_second_upper = upper_limit;
    state.chars_per_second_lower = lower_limit;
}

fn load_previous_human_response_times() -> Vec<f32>{
    let file = match File::open("response_times.txt") {
        Ok(f) => f,
        Err(e) => {
            println!("Failed to open file: {}. Trying to create it.", e);
            if let Err(e) = File::create("response_times.txt") {
                println!("Failed to create file: {}", e);
            }
            return vec![];
        }
    };

    let mut values: Vec<f32> = vec![];

    let reader = BufReader::new(file);
    for line in reader.lines(){
        match line{
            Ok(l) => {
                match l.parse::<f32>(){
                    Ok(v) => {
                        values.push(v);
                    },
                    Err(e) => {
                        println!("Failed to parse line: {}", e);
                    }
                }
            },
            Err(e) => {
                println!("Failed to read line: {}", e);
            }
        }
    }

    values
}

fn save_human_response_times_to_file(values: &Vec<f32>){
    println!("Saving response times to file");
    let file = match File::create("response_times.txt") {
        Ok(f) => f,
        Err(e) => {
            println!("Failed to create file: {}", e);
            return;
        }
    };

    let mut writer = BufWriter::new(file);

    for v in values{
        match writeln!(writer, "{}", v){
            Ok(_) => {},
            Err(e) => {
                println!("Failed to write line: {}", e);
            }
        }
    }
}