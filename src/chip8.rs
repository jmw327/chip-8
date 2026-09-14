use crate::font;
use rand::RngExt;
use std::{fs, path::Path};

pub const PIXELS: usize = 64 * 32;

pub struct Chip8 {
    registers: [u8; 16],
    memory: [u8; 4096],
    // from 0x000 to 0xFFF
    index_register: u16,
    // from 0x000 to 0xFFF
    pc: u16,
    stack: [u16; 16],
    sp: usize,
    delay_timer: u8,
    sound_timer: u8,
    screen: [u8; PIXELS],
    keys: [bool; 16],
    waiting_on_key: bool,
    key_register: usize,
}

impl Chip8 {
    pub fn new() -> Self {
        // load font into memory
        let mut memory = [0u8; 4096];
        memory[font::START..font::START + font::DATA.len()]
            .copy_from_slice(&font::DATA);

        Self {
            registers: [0u8; 16],
            memory,
            index_register: 0u16,
            pc: 0x200,
            stack: [0u16; 16],
            sp: 0,
            delay_timer: 0u8,
            sound_timer: 0u8,
            screen: [0u8; PIXELS],
            keys: [false; 16],
            waiting_on_key: false,
            key_register: 0,
        }
    }

    pub fn load(&mut self, path: &Path) -> anyhow::Result<()> {
        let bytes = fs::read(path)?;
        let start = 0x200;
        let max_size = self.memory.len() - start;

        anyhow::ensure!(
            bytes.len() <= max_size,
            "rom exceeds {max_size}B memory limit"
        );

        self.memory[start..start + bytes.len()].copy_from_slice(&bytes);

        Ok(())
    }

    pub fn set_key(&mut self, key: usize, pressed: bool) {
        let newly_pressed = pressed && !self.keys[key];
        self.keys[key] = pressed;
        if self.waiting_on_key && newly_pressed {
            self.waiting_on_key = false;
            self.registers[self.key_register] = key as u8;
        }
    }

    pub fn emulate(&mut self) {
        if self.waiting_on_key {
            return;
        }

        // fetch opcode
        let opcode: u16 = ((self.memory[self.pc as usize] as u16) << 8)
            | (self.memory[self.pc as usize + 1] as u16);

        let x = ((opcode >> 8) & 0xF) as usize;
        let y = ((opcode >> 4) & 0xF) as usize;
        let n = opcode & 0xF;
        let nn = (opcode & 0xFF) as u8;
        let nnn = opcode & 0xFFF;

        let vx = self.registers[x];
        let vy = self.registers[y];

        self.pc += 2;

        // decode opcode and execute
        match opcode & 0xF000 {
            0x0000 => {
                match opcode & 0x00FF {
                    // 0x00E0 clears the screen
                    0x00E0 => {
                        self.screen = [0u8; PIXELS];
                    }
                    // 0x00EE returns from subroutine
                    0x00EE => {
                        if self.sp > 0 {
                            self.sp -= 1;
                            self.pc = self.stack[self.sp];
                            self.stack[self.sp] = 0;
                        }
                    }
                    _ => println!("Unknown OpCode: 0x{:04x}", opcode),
                }
            }
            // 0x1NNN jumps to address NNN
            0x1000 => {
                self.pc = opcode & 0x0FFF;
            }
            // 0x2NNN calls subroutine at address NNN
            0x2000 => {
                self.stack[self.sp] = self.pc;
                self.sp += 1;

                self.pc = opcode & 0x0FFF;
            }
            // 0x3XNN skip next instruction if VX == NN
            0x3000 => {
                if vx == nn {
                    self.pc += 2;
                }
            }
            // 0x4XNN skip next instruction if VX != NN
            0x4000 => {
                if vx != nn {
                    self.pc += 2;
                }
            }
            // 0x5XNN skip next instruction if VX == VY
            0x5000 => {
                if vx == vy {
                    self.pc += 2;
                }
            }
            // 0x6XNN sets register X to bye NN
            0x6000 => {
                self.registers[x] = nn;
            }
            // 0x7XNN adds NN to register X
            0x7000 => {
                self.registers[x] = vx.wrapping_add(nn);
            }
            0x8000 => {
                match opcode & 0x000F {
                    0x0000 => {
                        self.registers[x] = vy;
                    }
                    0x0001 => {
                        self.registers[x] = vx | vy;
                    }
                    0x0002 => {
                        self.registers[x] = vx & vy;
                    }
                    0x0003 => {
                        self.registers[x] = vx ^ vy;
                    }
                    // 0x8XY4: adds VY to VX
                    0x0004 => {
                        let (result, carry) = vx.overflowing_add(vy);
                        self.registers[x] = result;
                        self.registers[0xF] = u8::from(carry);
                    }
                    0x0005 => {
                        let (result, borrowed) = vx.overflowing_sub(vy);
                        self.registers[x] = result;
                        self.registers[0xF] = u8::from(!borrowed);
                    }
                    0x0006 => {
                        self.registers[x] = vx >> 1;
                        self.registers[0xF] = vx & 1;
                    }
                    0x0007 => {
                        let (result, borrowed) = vy.overflowing_sub(vx);
                        self.registers[x] = result;
                        self.registers[0xF] = u8::from(!borrowed);
                    }
                    0x000E => {
                        self.registers[x] = vx << 1;
                        self.registers[0xF] = vx >> 7;
                    }
                    _ => println!("Unknown OpCode: 0x{:04x}", opcode),
                }
            }
            0x9000 => {
                if vx != vy {
                    self.pc += 2;
                }
            }
            // opcode ANNN sets the index register to NNN
            0xA000 => {
                self.index_register = nnn;
            }
            0xB000 => {
                self.pc = nnn + self.registers[0] as u16;
            }
            0xC000 => {
                let random_byte: u8 = rand::rng().random();
                self.registers[x] = random_byte & nn;
            }
            // 0xDXYN draws sprite at position vx, vy with n pixel height
            // and 8 pixel width starting from memory location specified in
            // the index register
            0xD000 => {
                self.registers[0xF] = 0;

                let start = self.index_register;
                for row in start..start + n {
                    let byte = self.memory[row as usize];
                    for col in (0..8).rev() {
                        let bit = (byte >> col) & 1;

                        let x_offset = 7 - col;
                        let y_offset = row - start;
                        self.set_pixel(
                            (vx as usize % 64) + x_offset as usize,
                            (vy as usize % 32) + y_offset as usize,
                            bit,
                        );
                    }
                }
            }
            0xE000 => {
                let key = self.registers[x] & 0xF;
                match opcode & 0x00FF {
                    0x009E => {
                        if self.keys[key as usize] {
                            self.pc += 2;
                        }
                    }
                    0x00A1 => {
                        if !self.keys[key as usize] {
                            self.pc += 2;
                        }
                    }
                    _ => println!("Unknown OpCode: 0x{:04x}", opcode),
                }
            }
            0xF000 => {
                match nn {
                    0x07 => {
                        self.registers[x] = self.delay_timer;
                    }
                    0x0A => {
                        self.waiting_on_key = true;
                        self.key_register = x;
                    }
                    0x15 => {
                        self.delay_timer = vx;
                    }
                    0x18 => {
                        self.sound_timer = vx;
                    }
                    0x1E => {
                        self.index_register += vx as u16;
                    }
                    0x29 => {
                        let address = font::START + (vx * 5) as usize;
                        self.index_register = address as u16;
                    }
                    // 0xFX33 stores BCD of VX in I, I + 1, I + 2
                    0x33 => {
                        let i = self.index_register;
                        self.memory[i as usize] = vx / 100;
                        self.memory[i as usize + 1] = (vx / 10) % 10;
                        self.memory[i as usize + 2] = vx % 10;
                    }
                    0x55 => {
                        let start = self.index_register as usize;
                        let end = start + x + 1;
                        self.memory[start..end]
                            .copy_from_slice(&self.registers[..=x]);
                    }
                    0x65 => {
                        let start = self.index_register as usize;
                        let end = start + x + 1;
                        self.registers[..=x]
                            .copy_from_slice(&self.memory[start..end]);
                    }
                    _ => println!("Unknown OpCode: 0x{:04x}", opcode),
                }
            }
            _ => println!("Unknown OpCode: 0x{:04x}", opcode),
        }
    }

    pub fn tick_timers(&mut self) {
        self.delay_timer = self.delay_timer.saturating_sub(1);
        self.sound_timer = self.sound_timer.saturating_sub(1);
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, value: u8) {
        if x >= 64 || y >= 32 {
            return;
        }

        let index = y * 64 + x;
        if self.screen[index] == 1 && value == 1 {
            self.registers[0xF] = 1;
        }
        self.screen[index] ^= value;
    }

    pub fn screen(&self) -> &[u8] {
        &self.screen
    }
}
