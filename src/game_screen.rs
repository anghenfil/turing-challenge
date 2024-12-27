use std::time::SystemTime;
use eframe::egui::{Align, Color32, Context, Key, KeyboardShortcut, Margin, Modifiers, RichText, ScrollArea, Separator, TextEdit};
use eframe::{egui, Frame};
use egui_extras::{Size, StripBuilder};
use rand::Rng;
use crate::{ApplicationState, ChatMessage, ChatMessageOrigin, InterTaskMessageToNetworkTask, PlayerMessage, TcpMessage};

pub fn render_game_screen(app: &mut ApplicationState, ctx: &Context, frame: &mut Frame) {
    let time_elapsed = app.game_start_time.unwrap().elapsed().unwrap().as_secs();
    let time = if time_elapsed >= 210 {
        0
    } else {
        210 - time_elapsed
    };

    // let the llm send the first message if the other player has not sent a message after a certain time
    if !app.llm_chat_first_message && time_elapsed >= app.llm_take_iniative_after as u64 {
        app.llm_chat_first_message = true;
        app.llm_last_message_time = Some(SystemTime::now());

        app.mpsc_sender.send(InterTaskMessageToNetworkTask::ContactLLM {
            msg: PlayerMessage {
                msg: "Schreibe einen kurzen Konservationsstarter, lasse dir nicht anmerken, dass du eine KI bist. Reagiere nicht auf diese Nachricht. Schreibe nur etwas wie Hey! oder Hey, wie gehts dir? oder Hallo oder Moin oder ähnliches".to_string(),
                from_ai: false,
                to_ai: false,
                timestamp: SystemTime::now(),
            },
            history: app.llm_history.clone(),
            client: app.reqwest_client.clone(),
            settings: app.settings.clone(),
        }).expect("Channel to network task is closed :(");
    }

    if let Some(last_msg_time) = app.llm_last_message_time {
        // let the llm send a message if the other player has not sent a message after a certain time
        if last_msg_time.elapsed().unwrap().as_secs() >= app.llm_noresponse_iniative_time as u64 {
            println!("No response from other player, sending message");
            app.mpsc_sender.send(InterTaskMessageToNetworkTask::ContactLLM {
                msg: PlayerMessage {
                    msg: "Dein Gesprächspartner hat jetzt länger nicht geantwortet. Reagiere nicht auf diese Nachricht, sondern auf die Nachricht davor. Schreibe eine kurze Nachfrage wie Hallo?; Noch da?; ?.".to_string(),
                    from_ai: false,
                    to_ai: false,
                    timestamp: SystemTime::now(),
                },
                history: app.llm_history.clone(),
                client: app.reqwest_client.clone(),
                settings: app.settings.clone(),
            }).expect("Channel to network task is closed :(");
            app.llm_last_message_time = Some(SystemTime::now());
        }
    }

    egui::CentralPanel::default().show(ctx, |ui| {
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        let total_width = ui.available_width();
        let content_width = total_width * 0.9;
        let side_width = (total_width - content_width) / 2.0;

        StripBuilder::new(ui)
            .size(Size::exact(side_width))
            .size(Size::exact(content_width))
            .size(Size::exact(side_width))
            .horizontal(|mut strip| {
                strip.cell(|ui| {});
                strip.cell(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.heading("The Turing Challenge");
                        ui.add_space(10.0);
                        StripBuilder::new(ui)
                            .size(Size::exact((content_width / 2.0) - 15.0))
                            .size(Size::exact(10.0))
                            .size(Size::exact((content_width / 2.0) - 15.0))
                            .horizontal(|mut strip| {
                                strip.cell(|ui| {
                                    let mut frame = egui::Frame::default();
                                    frame = frame.inner_margin(Margin::same(5.0)).fill(Color32::from_hex("#29114C").unwrap());
                                    frame.show(ui, |ui| {
                                        ui.label("Chat #1:");
                                        let history_space = ui.available_height() - 80.0;
                                        ScrollArea::vertical().stick_to_bottom(true).max_height(history_space).show(ui, |ui| {
                                            let max_width = ui.available_width();
                                            ui.set_width(max_width);

                                            // Show chat history
                                            show_messages(ui, &app.chat1_history, max_width);
                                        });
                                        ui.separator();

                                        let current_chat_input = app.chat1_input.clone();

                                        let mut text_edit = TextEdit::multiline(&mut app.chat1_input);
                                        text_edit = text_edit.background_color(Color32::from_hex("#190B2F").unwrap());
                                        text_edit = text_edit.desired_width(ui.available_width());
                                        text_edit = text_edit.return_key(Some(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter)));

                                        let mut submit_msg = false;

                                        ScrollArea::vertical().id_salt("input_scrollarea").max_height(50.0).show(ui, |ui| {
                                            let response = ui.add(text_edit);

                                            if response.has_focus() && ui.input_mut(|i| { i.consume_key(Modifiers::NONE, Key::Enter) && !i.modifiers.contains(Modifiers::SHIFT) }) {
                                                // Enter pressed, send message
                                                submit_msg = true;
                                            }
                                        });

                                        if submit_msg && !app.chat1_input.is_empty() {
                                            let msg_text = current_chat_input;
                                            // Add message to chat history
                                            app.chat1_history.push(ChatMessage {
                                                timestamp: SystemTime::now(),
                                                message: msg_text.clone(),
                                                from: ChatMessageOrigin::Own,
                                            });

                                            let to_ai = if app.human_chat == 0 { //This is the chat to a human
                                                false
                                            } else {
                                                true
                                            };

                                            // Send message to network task
                                            let tcp_msg = TcpMessage::Message(PlayerMessage {
                                                msg: msg_text,
                                                from_ai: false,
                                                to_ai,
                                                timestamp: SystemTime::now(),
                                            });

                                            app.mpsc_sender.send(InterTaskMessageToNetworkTask::SendMsg { msg: tcp_msg }).expect("Channel to network task is closed :(");
                                            app.chat1_input = "".to_string();
                                        }
                                    });
                                });
                                strip.cell(|ui| {
                                    ui.add(Separator::default().vertical());
                                });
                                strip.cell(|ui| {
                                    let mut frame = egui::Frame::default();
                                    frame = frame.inner_margin(Margin::same(5.0)).fill(Color32::from_hex("#29114C").unwrap());
                                    frame.show(ui, |ui| {
                                        ui.label("Chat #2:");
                                        let history_space = ui.available_height() - 80.0;

                                        ScrollArea::vertical().stick_to_bottom(true).max_height(history_space).show(ui, |ui| {
                                            let max_width = ui.available_width();
                                            ui.set_width(max_width);

                                            // Show chat history
                                            show_messages(ui, &app.chat2_history, max_width);
                                        });
                                        ui.separator();
                                        let current_chat_input = app.chat2_input.clone();

                                        let mut text_edit = TextEdit::multiline(&mut app.chat2_input);
                                        text_edit = text_edit.background_color(Color32::from_hex("#190B2F").unwrap());
                                        text_edit = text_edit.desired_width(ui.available_width());
                                        text_edit = text_edit.return_key(Some(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter)));

                                        let mut submit_msg = false;

                                        ScrollArea::vertical().id_salt("input_scrollarea2").max_height(50.0).show(ui, |ui| {
                                            let response = ui.add(text_edit);

                                            if response.has_focus() && ui.input_mut(|i| { i.consume_key(Modifiers::NONE, Key::Enter) && !i.modifiers.contains(Modifiers::SHIFT) }) {
                                                // Enter pressed, send message

                                                submit_msg = true;
                                            }
                                        });

                                        if submit_msg && !app.chat2_input.is_empty() {
                                            let msg_text = current_chat_input;
                                            // Add message to chat history
                                            app.chat2_history.push(ChatMessage {
                                                timestamp: SystemTime::now(),
                                                message: msg_text.clone(),
                                                from: ChatMessageOrigin::Own,
                                            });

                                            let to_ai = if app.human_chat == 1 { //This is the chat to a human
                                                false
                                            } else {
                                                true
                                            };

                                            // Send message to network task
                                            let tcp_msg = TcpMessage::Message(PlayerMessage {
                                                msg: msg_text,
                                                timestamp: SystemTime::now(),
                                                from_ai: false,
                                                to_ai,
                                            });
                                            app.mpsc_sender.send(InterTaskMessageToNetworkTask::SendMsg { msg: tcp_msg }).expect("Channel to network task is closed :(");

                                            app.chat2_input = "".to_string();
                                        }
                                    });
                                });
                            });
                    });
                });
                strip.cell(|ui| {
                    ui.horizontal(|ui| {
                        let mut frame = egui::Frame::default();
                        frame = frame.fill(egui::Color32::from_hex("#29114C").unwrap());
                        frame.show(ui, |ui| {
                            let mut label = RichText::new(format!("{}", time));
                            if time <= 15{
                                if time % 2 == 0 {
                                    label = label.strong();
                                }
                            }
                            ui.add_sized([30.0, 25.0], egui::Label::new(label));
                        });
                    });
                });
            });
    });
}

pub fn show_message_frame(ui: &mut egui::Ui, msg: &ChatMessage){
        let mut msg_frame = egui::Frame::default();
        msg_frame = msg_frame.fill(Color32::from_hex("#B2AAFF").unwrap());
        msg_frame = msg_frame.inner_margin(Margin::same(5.0));

        msg_frame.show(ui, |ui|{
            let label = egui::Label::new(RichText::new(msg.message.clone()).color(Color32::BLACK)).wrap();
            ui.add(label);
        });
    }

    pub fn show_messages(ui: &mut egui::Ui, msgs: &Vec<ChatMessage>, max_width: f32){
        for msg in msgs.iter(){
            match msg.from{
                ChatMessageOrigin::Own => {
                    ui.allocate_ui_with_layout([max_width*0.8, 10.0].into(), egui::Layout::right_to_left(Align::Min), |ui|{
                        show_message_frame(ui, msg)
                    });
                },
                ChatMessageOrigin::Foreign => {
                    ui.allocate_ui_with_layout([max_width*0.8, 10.0].into(), egui::Layout::left_to_right(Align::Min), |ui|{
                        show_message_frame(ui, msg)
                    });
                },
            };
        }
        ui.add_space(ui.available_height());
    }