use learn_asyn_rust::chapters::{ch4};
fn main() {
    //ch3::channel_multiple_producers();
    ch4::block_on(ch4::CountToTwo{
        polled_once: false}
    );
}