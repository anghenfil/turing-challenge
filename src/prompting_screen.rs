use eframe::egui::{Context, RichText, ScrollArea, TextEdit};
use eframe::{egui, Frame};
use egui_extras::{Size, StripBuilder};
use crate::{ApplicationState, InterTaskMessageToNetworkTask, TcpMessage};

pub fn render_prompting_screen(app: &mut ApplicationState, ctx: &Context, frame: &mut Frame){
    let time_elapsed = app.prompting_start_time.unwrap().elapsed().unwrap().as_secs();
    let time = if time_elapsed >= 90 {
        0
    }else{
        90 - time_elapsed
    };


    egui::CentralPanel::default().show(ctx, |ui| {
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
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
                        ui.heading("Start Prompting!");
                        ui.add_space(10.0);
                        ui.label("Initial Prompt");
                        ScrollArea::vertical().max_height(400.0).show(ui, |ui|{
                            let mut text_edit = TextEdit::multiline(&mut app.custom_prompt);
                            if app.marked_as_prompt_ready{
                                text_edit = text_edit.interactive(false);
                            }
                            ui.add_sized([ui.available_width(), 400.0], text_edit);
                        });
                        ui.add_space(10.0);
                        let button = egui::Button::new("Submit Prompt");

                        if app.marked_as_prompt_ready{
                            ui.add_enabled(false, button);
                            ui.spinner();
                        }else{
                            if ui.add(button).clicked(){
                                app.marked_as_prompt_ready = true;
                                app.mpsc_sender.send(InterTaskMessageToNetworkTask::SendMsg { msg: TcpMessage::PromptingFinished }).expect("Failed to send message to network task");
                            }
                        }
                    });
                });
                strip.cell(|ui|{
                    ui.horizontal(|ui|{
                        let available_width = ui.available_width();
                        ui.add_space((available_width - 30.0)/2.0);
                        let mut frame = egui::Frame::default();
                        frame = frame.fill(egui::Color32::from_hex("#29114C").unwrap());
                        frame.show(ui, |ui|{
                            let mut time_left = RichText::new(format!("{}", time));

                            if time < 15 && time % 2 == 0 {
                                time_left = time_left.strong();
                            }

                            ui.add_sized([30.0, 25.0], egui::Label::new(time_left));
                        });
                        ui.add_space((available_width - 30.0)/2.0);
                    });
                });
            });
    });
}