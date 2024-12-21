use std::sync::Arc;
use iced::{color, Border, Color, Element, Theme};
use iced::border::Radius;
use tokio_rustls::rustls::server::WebPkiClientVerifier;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use crate::certs::{load_client_cert, load_private_key, load_root_ca};

pub mod ConnectScreen;
pub mod certs;
pub mod settings;
#[derive(Debug, Clone, Default)]
pub enum Screen {
    #[default]
    Connect,
    Start,
    Game,
    End,
}

#[derive(Debug, Default)]
pub struct ApplicationState {
    pub screen: Screen,
    pub connect_host: String,
    pub name: String,
    pub custom_prompt: Option<String>,
}

#[derive(Debug, Clone)]
enum Message{
    IpSet(String),
    ConnectButtonPressed,
    NameChanged(String),
    CustomPromptChanged(String),
    ChangeScreen(Screen),
}

impl ApplicationState {
    fn update(&mut self, message: Message){
        match message{
            Message::IpSet(ip) => {
                self.connect_host = ip;
            }
            Message::NameChanged(name) => {
                self.name = name;
            }
            Message::CustomPromptChanged(custom_prompt) => {
                self.custom_prompt = Some(custom_prompt);
            },
            Message::ChangeScreen(screen) => {
                self.screen = screen;
            }
            Message::ConnectButtonPressed => {
                //TODO
            }
        }
    }

    fn view(&self) -> Element<Message> {
        match self.screen{
            Screen::Connect => {
                ConnectScreen::view(self)
            }
            _ => {
                 unimplemented!("Screen not implemented")
            }
        }
    }
}

struct c3Theme;

static COLOR_PRIMARY: Color = color!(0xFF5053);
static COLOR_HIGHLIGHT: Color = color!(0xFEF2FF);
static COLOR_BACKGROUND: Color = color!(0x0F000A);

impl c3Theme{
    fn theme(state: &ApplicationState) -> Theme{
        let c3palette = iced::theme::Palette{
            background: COLOR_BACKGROUND,
            text: COLOR_PRIMARY,
            primary: color!(0x6A5FDB),
            success: color!(0xB2AAFF),
            danger: color!(0xFEF2FF),
        };
        Theme::custom("38c3".to_string(), c3palette)
    }

    fn style_button(theme: &Theme, status: iced::widget::button::Status) -> iced::widget::button::Style{
        iced::widget::button::Style{

            background: None,
            text_color: COLOR_PRIMARY,
            border: Border{
                color: COLOR_PRIMARY,
                width: 1.0,
                radius: Radius::new(3),
            },
            shadow: Default::default(),
        }

    }
}

#[tokio::main]
async fn main()  {
    let settings : Arc<settings::Settings> = Arc::new(settings::Settings::new().expect("Couldn't read config(s)!"));

    // Load mtls certs
    let root_ca = Arc::new(load_root_ca("root.crt".to_string()));
    let client_cert = load_client_cert("client.crt".to_string());
    let client_key = load_private_key("client.key".to_string());

    // Server Config
    let client_verifier = WebPkiClientVerifier::builder(root_ca.clone()).build().expect("Couldn't build Client Verifier. Check Certs & Key!");

    let server_config = ServerConfig::builder_with_protocol_versions(&[&tokio_rustls::rustls::version::TLS13])
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(client_cert.clone(), client_key).expect("Couldn't build Server Config. Check Certs & Key!");

    // Create Server to listen on incoming rendering requests
    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = TcpListener::bind(format!("{}:{}", settings.bind_to_host, settings.port)).await.unwrap();

    iced::application("Turing Challenge", ApplicationState::update, ApplicationState::view).theme(c3Theme::theme).run();
}
