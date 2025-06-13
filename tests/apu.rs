use vibeEmu::apu::Apu;

#[test]
fn frame_sequencer_tick() {
    let mut apu = Apu::new();
    assert_eq!(apu.sequencer_step(), 0);
    apu.step(8192);
    assert_eq!(apu.sequencer_step(), 1);
    apu.step(8192 * 7);
    assert_eq!(apu.sequencer_step(), 0);
}
