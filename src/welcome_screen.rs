use crate::egui::RichText;
use eframe::egui::{Button, Color32, Context, Label, Pos2, Rect, ScrollArea, TextEdit, Vec2};
use eframe::{egui, Frame};
use egui_extras::{Size, StripBuilder};
use crate::{ApplicationState, InterTaskMessageToNetworkTask, TcpMessage};

pub fn render_welcome_screen(app: &mut ApplicationState, ctx: &Context, frame: &mut Frame){
    egui::CentralPanel::default().show(ctx, |ui| {
        let total_width = ui.available_width();
        let content_width = total_width*0.6;
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
                        ui.heading("Welcome to the Turing Challenge");
                        ui.add_space(10.0);
                        ui.allocate_ui_with_layout(Vec2::from([content_width, 20.0]), egui::Layout::left_to_right(egui::Align::Min), |ui|{
                            let mut label = Label::new(RichText::new("You will see two Chats, one belongs to the other human, the other one to an LLM. You will have 3,5 minutes to find out which one is which!\nYou may change the initial prompt of the LLM your opponent will encounter (max 1,5 minutes)."));
                            label = label.wrap();
                            ui.add(label);
                        });
                        ui.add_space(10.0);
                        ui.horizontal(|ui|{
                            ui.label("Username");
                            let mut text_edit = TextEdit::singleline(&mut app.name);

                            if app.marked_as_ready{
                                text_edit = text_edit.interactive(false);
                            }

                            ui.add_sized(ui.available_size(), text_edit);
                        });
                        ui.add_space(10.0);
                        let button = egui::Button::new("Mark as Ready");

                        if app.marked_as_ready{
                            ui.add_enabled(false, button);
                            ui.spinner();
                        }else{
                            if ui.add(button).clicked(){
                                app.marked_as_ready = true;
                                app.mpsc_sender.send(InterTaskMessageToNetworkTask::SendMsg { msg: TcpMessage::MarkedAsReady }).expect("Channel to network task is closed :(");
                            }
                        }
                    });
                });
                strip.cell(|ui|{

                });
            });
    });
}