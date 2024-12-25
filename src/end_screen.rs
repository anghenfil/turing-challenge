use crate::egui::RichText;
use std::time::SystemTime;
use eframe::egui::{Color32, Context, Key, KeyboardShortcut, Margin, Modifiers, ScrollArea, Separator, TextEdit, TextStyle};
use eframe::{egui, Frame};
use egui_extras::{Size, StripBuilder};
use crate::{ApplicationState, ChatMessage, ChatMessageOrigin, InterTaskMessageToNetworkTask, PlayerMessage, TcpMessage};

pub fn render_end_screen(app: &mut ApplicationState, ctx: &Context, frame: &mut Frame){
    egui::CentralPanel::default().show(ctx, |ui| {
        let total_width = ui.available_width();
        let content_width = total_width*0.9;
        let side_width = (total_width - content_width)/2.0;

        StripBuilder::new(ui)
            .size(Size::exact(side_width))
            .size(Size::exact(content_width))
            .size(Size::exact(side_width))
            .horizontal(|mut strip|{
                strip.cell(|ui|{

                });
                strip.cell(|ui|{
                    ui.vertical_centered(|ui|{
                        ui.add_space(10.0);
                        ui.heading("Which Chat belongs to the AI?");
                        ui.add_space(10.0);
                        StripBuilder::new(ui)
                            .size(Size::exact((content_width/2.0)-15.0))
                            .size(Size::exact(10.0))
                            .size(Size::exact((content_width/2.0)-15.0))
                            .horizontal(|mut strip|{
                                strip.cell(|ui|{
                                    let mut frame = egui::Frame::default();
                                    frame = frame.inner_margin(Margin::same(5.0)).fill(Color32::from_hex("#261A66").unwrap());
                                    frame.show(ui, |ui|{
                                        ui.label("Chat #1:");
                                        let history_space = ui.available_height() - 80.0;
                                        ScrollArea::vertical().stick_to_bottom(true).max_height(history_space).show(ui, |ui|{
                                            let max_width = ui.available_width();
                                            ui.set_width(max_width);

                                            // Show chat history
                                            crate::game_screen::show_messages(ui, &app.chat1_history, max_width);
                                        });
                                        ui.separator();
                                        if ui.button("I think this is the AI chat").clicked(){
                                            if app.human_chat == 1{
                                                // Correct
                                                app.correctly_guessed = Some(true);
                                                println!("Player guessed correctly");
                                            }else{
                                                // Incorrect
                                                app.correctly_guessed = Some(false);
                                                println!("Player guessed incorrectly");
                                            }
                                            app.screen = crate::Screen::End2;
                                            app.showing_end_screen_since = Some(SystemTime::now())
                                        }

                                    });
                                });
                                strip.cell(|ui|{
                                    ui.add(Separator::default().vertical());
                                });
                                strip.cell(|ui|{
                                    let mut frame = egui::Frame::default();
                                    frame = frame.inner_margin(Margin::same(5.0)).fill(Color32::from_hex("#261A66").unwrap());
                                    frame.show(ui, |ui|{
                                        ui.label("Chat #2:");
                                        let history_space = ui.available_height() - 80.0;

                                        ScrollArea::vertical().stick_to_bottom(true).max_height(history_space).show(ui, |ui|{
                                            let max_width = ui.available_width();
                                            ui.set_width(max_width);

                                            // Show chat history
                                            crate::game_screen::show_messages(ui, &app.chat2_history, max_width);
                                        });
                                        ui.separator();
                                        if ui.button("I think this is the AI chat").clicked(){
                                            if app.human_chat == 0{
                                                // Correct
                                                app.correctly_guessed = Some(true);
                                                println!("Player guessed correctly");
                                            }else{
                                                // Incorrect
                                                app.correctly_guessed = Some(false);
                                                println!("Player guessed incorrectly");
                                            }
                                            app.screen = crate::Screen::End2;
                                            app.showing_end_screen_since = Some(SystemTime::now())
                                        }
                                    });
                                });
                            });
                    });
                });
                strip.cell(|ui|{

                });
            });
    });
}

pub fn render_end_screen2(app: &mut ApplicationState, ctx: &Context, frame: &mut Frame){
    egui::CentralPanel::default().show(ctx, |ui| {
        let total_width = ui.available_width();
        let content_width = total_width*0.9;
        let side_width = (total_width - content_width)/2.0;

        StripBuilder::new(ui)
            .size(Size::exact(side_width))
            .size(Size::exact(content_width))
            .size(Size::exact(side_width))
            .horizontal(|mut strip|{
                strip.cell(|ui|{

                });
                strip.cell(|ui|{
                    ui.vertical_centered(|ui|{
                        ui.add_space(10.0);
                        ui.heading("The Turing Challenge");
                        ui.add_space(30.0);
                        let text = if app.correctly_guessed.unwrap(){
                            "Congratulations! You have successfully identified the AI chat."
                        }else{
                            "Sorry, you have failed to identify the AI chat :("
                        };

                        ui.label(RichText::from(text).size(40.0));
                    });
                });
                strip.cell(|ui|{

                });
            });
    });
}