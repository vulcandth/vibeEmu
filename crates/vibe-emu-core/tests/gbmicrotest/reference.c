/* Independent fixture capture for gbmicrotest_probes.md.
 * Build against SameBoy 1.0.2, not vibeEmu. This is not part of cargo test.
 */
#include "Core/gb.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static GB_gameboy_t gb;
static uint32_t pixels[256 * 224];
static unsigned frames;
static bool initialized;

static uint32_t rgb(GB_gameboy_t *g, uint8_t r, uint8_t green, uint8_t b)
{
    (void)g;
    return (r << 16) | (green << 8) | b;
}

static void execution(GB_gameboy_t *g, uint16_t pc, uint8_t op)
{
    (void)pc;
    (void)op;
    if (g->boot_rom_finished && !initialized) {
        /* Match vibeEmu's deterministic OAM initialization. The boot ROM does
         * not initialize OAM, and several exploratory probes omit that step. */
        memset(g->oam, 0, 160);
        initialized = true;
    }
}

static void frame(GB_gameboy_t *g, GB_vblank_type_t type)
{
    if (g->boot_rom_finished && type == GB_VBLANK_TYPE_NORMAL_FRAME) frames++;
}

static void save(const char *prefix, const char *suffix, const void *data, size_t size)
{
    char path[4096];
    int length = snprintf(path, sizeof(path), "%s.%s", prefix, suffix);
    if (length < 0 || (size_t)length >= sizeof(path)) {
        fprintf(stderr, "Output path too long\n");
        exit(1);
    }
    FILE *file = fopen(path, "wb");
    if (!file || fwrite(data, 1, size, file) != size || fclose(file)) {
        fprintf(stderr, "Could not write %s\n", path);
        exit(1);
    }
}

int main(int argc, char **argv)
{
    if (argc != 4) {
        fprintf(stderr, "Usage: %s <rom.gb> <dmg_boot.bin> <output-prefix>\n", argv[0]);
        return 1;
    }
    GB_init(&gb, GB_MODEL_DMG_B);
    GB_set_rgb_encode_callback(&gb, rgb);
    GB_set_pixels_output(&gb, pixels);
    GB_set_turbo_mode(&gb, true, false);
    if (GB_load_boot_rom(&gb, argv[2]) || GB_load_rom(&gb, argv[1])) {
        fprintf(stderr, "Could not load ROM/boot ROM\n");
        return 1;
    }
    GB_set_vblank_callback(&gb, frame);
    GB_set_execution_callback(&gb, execution);
    GB_set_palette(&gb, &GB_PALETTE_GREY);

    /* GB_run returns 8 MHz clocks: four million is two million DMG dots. */
    unsigned long long elapsed = 0;
    for (unsigned long long i = 0; i < 200000000 && frames < 6 && elapsed < 4000000; i++) {
        unsigned ticks = GB_run(&gb);
        if (gb.boot_rom_finished) elapsed += ticks;
    }
    if (frames != 6) {
        fprintf(stderr, "No observation frame: frames=%u pc=%04X\n", frames, gb.pc);
        return 1;
    }
    uint8_t gray[160 * 144];
    for (size_t i = 0; i < sizeof(gray); i++) gray[i] = pixels[i] >> 16;
    save(argv[3], "gray", gray, sizeof(gray));
    save(argv[3], "vram", gb.vram, 8192);
    save(argv[3], "oam", gb.oam, 160);
    GB_free(&gb);
    return 0;
}
