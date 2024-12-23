use iced::{Element, Fill, Length};
use iced::advanced::Widget;
use iced::widget::{container, column, text, Space, text_input, text_editor, row, button, TextInput};
use crate::{ApplicationState, Message};

pub fn view(state: &ApplicationState) -> Element<Message>{
    let mut column = column![];

    column = column.push(Space::with_height(50));
    column = column.push(text("Welcome to the Turing Challenge!").size(30));
    column = column.push(Space::with_height(50));
    column = column.push(text("Please enter your username:").size(20));
    column = column.push(Space::with_height(10));
    column = column.push(text_input("username", &state.name).on_input(Message::NameChanged));
    column = column.push(Space::with_height(10));
    column = column.push(text("Enter a custom prompt for the AI the other player will face:"));
    column = column.push(Space::with_height(10));
    column = column.push(text_editor(&state.custom_prompt).on_action(Message::CustomPromptChanged).height(200));
    column = column.push(Space::with_height(10));
    column = column.push(button("Mark as Ready").on_press(Message::MarkedAsReady)).width(Length::Shrink);
    column = column.width(Length::FillPortion(3));

    let side_left = Space::new(Length::FillPortion(1), Length::Fill);
    let side_right = Space::new(Length::FillPortion(1), Length::Fill);
    let main_row = row![side_left, column, side_right];

    let inner_container : Element<'_, Message>= container(main_row).height(Length::Shrink).into();
    let outer_container = container(inner_container).center(Fill).into();
    outer_container
}