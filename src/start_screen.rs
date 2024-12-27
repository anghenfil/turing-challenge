use crate::egui::RichText;
use eframe::egui::Context;
use eframe::{egui, Frame};
use egui_extras::{Size, StripBuilder};
use crate::{ApplicationState, InterTaskMessageToNetworkTask};

pub fn render_start_screen(app: &mut ApplicationState, ctx: &Context, frame: &mut Frame){
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
                       ui.heading("The Turing Challenge");
                       ui.add_space(40.0);

                       if let Some(warning) = &app.warning{
                           ui.label(warning).highlight();
                           ui.add_space(10.0);
                       }

                       if ui.button("Start Game").clicked(){
                           app.start_game_pressed = true;
                           app.mpsc_sender.send(InterTaskMessageToNetworkTask::ConnectTo {
                               host_string: app.settings.connect_to_host.clone(),
                           }).expect("Channel to network task is closed :(");
                       }

                       if app.start_game_pressed{
                           ui.spinner();
                       }

                       ui.add_space(50.0);
                       ui.label(RichText::new("Any Issues?\n Call 28000 for Support!"));
                   });
               });
               strip.cell(|ui|{

               });
            });
    });
}