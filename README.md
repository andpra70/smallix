# Smallix

Smallix e' un micro-sistema didattico in Rust con kernel `no_std` bootabile con GRUB su QEMU.

## Contenuto minimo

- `src/main.rs`: entry point, boot, panic handler.
- `src/arch/`: primitive x86_64 (I/O port, halt, reboot).
- `src/drivers/`: VGA text mode, seriale COM1, tastiera PS/2 polling.
- `src/userland/init.rs`: init minimale che avvia userland.
- `src/userland/sched.rs`: scheduler cooperativo process/thread POSIX-like.
- `src/userland/syscall.rs`: syscall layer minimale (`signal`, `wait`, `select`).
- `src/userland/dev.rs`: layer driver `/dev/*` (console, null, kmsg, net0, tty0).
- `src/drivers/net/rtl8139.rs`: driver NIC reale RTL8139 su bus PCI (QEMU).
- `src/drivers/ata.rs`: driver ATA PIO per block device persistente.
- `src/drivers/usb.rs`: driver block USB minimale (probe controller USB PCI + `/dev/usb0`).
- `src/userland/blockdev.rs`: layer blockdevice (`/dev/ramfs`, `/dev/hda`, `/dev/loop0`, `/dev/usb0`).
- `src/userland/fat32.rs`: backend FAT32 (auto-detect su block device, R/W file).
- `src/userland/vfs.rs`: VFS agnostico rispetto al device sottostante con fs su blocchi.
- `src/userland/shell.rs`: shell userland.
- `src/userland/commands/`: comandi modulari (`builtin`, `sys`, `fs`, `proc`, `net`).
- `root/`: seed del filesystem root (eccetto `/dev` e `/proc` pseudo-fs runtime).
- `boot/grub/grub.cfg`: configurazione bootloader.
- `config/`: file configurazione principali.
- `tools/`: script per build ISO/IMG e run su QEMU.

## Comandi shell disponibili

- `help`
- `echo <testo>`
- `clear`
- `uname`
- `lsdev`
- `cfg`
- `ps`
- `threads`
- `fork <proc_name> [exec_path [args]]`
- `exec <path> [args]`
- `execve <path> [args]`
- `exit [code]`
- `errno`
- `pthread <pid> <thread_name>`
- `kill <pid>`
- `signal <pid> <sig>`
- `wait [pid]`
- `select [timeout_ticks]`
- `schedtick [n]`
- `schedtest`
- `ls [path]`
- `cd <dir>`
- `pwd`
- `cat <path>`
- `touch <path>`
- `write <path> <text>`
- `rm <path>`
- `cp <src> <dst>`
- `mv <src> <dst>`
- `mount <source> [target]`
- `umount [target]`
- `mounts`
- `sh` (subshell, `exit` per tornare)
- `ifconfig [show|up|down|set <ip> <mask> <gw>]`
- `route [show|set-gw <ip>]`
- `ping <host|ip> [count]`
- `telnet <host> [port]`
- `telnet close <id>`
- `netstat`
- `halt`
- `reboot`

I comandi I/O passano dal layer `/dev/*`:

- `write /dev/console <msg>`
- `write /dev/kmsg <msg>`
- `cat /dev/net0`
- `cat /dev/ramfs`
- `cat /dev/hda`
- `cat /dev/loop0`
- `cat /dev/usb0`
- `lsdev` mostra device nodes.

Filesystem:

- path assoluti e relativi con `.` e `..`
- cwd persistente nel prompt (`smallix:/path$`)
- `mount /dev/ramfs [/mnt/ramfs]` monta un device RAM (deve contenere FAT32 valido)
- `mount /dev/hda [/mnt/hda]` monta fs persistente FAT32 su disco (QEMU `-hda`)
- `mount /dev/loop0 [/mnt/loop0]` monta una immagine embedded
- `mount /dev/usb0 [/mnt/usb0]` monta fs su block USB (richiede controller USB rilevato)
- `mount /images/rootfs.img [/mnt/loop0]` monta una immagine `.img` dal FS
- `/etc/mtab` e' aggiornato dal VFS a ogni mount
- i file comando in `/bin/*` possono essere caricati con `exec/execve`
- il seed rootfs viene copiato dalla cartella `root/` dentro `out/fat32.img` (`tools/mkfat32.sh`)
- VFS operativo in modalità FAT32
- FAT32 supporta R/W file e listing directory (no allocator, limiti correnti):
  - nomi `8.3` (es. `TEST.TXT`)
  - profondita' path fino a 2 segmenti (`/FILE.TXT`, `/DIR/FILE.TXT`)
  - dimensione `write` massima allineata ai limiti shell/VFS correnti
- `/proc` e' un pseudo-fs read-only con dati kernel/config:
  - `/proc/version`, `/proc/uptime`, `/proc/hostname`, `/proc/mounts`
  - `/proc/meminfo`, `/proc/sched`, `/proc/devices`
  - `/proc/net/dev`, `/proc/net/route`
  - `/proc/<pid>/status`, `/proc/<pid>/threads`
  - `/proc/config/{system,init,network,scheduler}` (file seed: `/etc/system.cfg`, `/etc/init.rc`, `/etc/net.cfg`, `/etc/sched.cfg`)

Scheduler limits correnti:

- processi: `64`
- thread: `256`

Rete:

- NIC reale QEMU `rtl8139` (non mock) con I/O frame raw.
- `ping` usa ARP + IPv4 + ICMP reali verso rete QEMU user-net.
- `telnet` usa handshake TCP minimale su rete reale.
- endpoint TCP di test: `10.0.2.100:2323` via `guestfwd` QEMU.

## Build e avvio

```bash
cd smallix
./tools/mkiso.sh
./tools/run-qemu.sh
```

Per immagine disco raw:

```bash
./tools/mkdisk.sh
./tools/run-qemu-hda.sh
```

Per avvio con disco USB emulato:

```bash
./tools/mkusbdisk.sh
./tools/run-qemu-usb.sh
```

Per boot con disco `hda` FAT32 pronto:

```bash
./tools/mkfat32.sh
./tools/run-qemu-fat32.sh
```

Test scheduler:

```bash
./tools/test-scheduler.sh
```

Test rete (ping reale su NIC QEMU):

```bash
./tools/test-network.sh
```

Test TCP (handshake reale su NIC QEMU):

```bash
./tools/test-tcp.sh
```

Test persistenza fs su blockdevice `/dev/hda` (due boot consecutivi):

```bash
./tools/test-persistence.sh
```

Avvio Smallix nel browser con TinyEMU/JSLinux:

```bash
./tools/setup-tinyemu-web.sh
python3 -m http.server 8000
# poi apri http://localhost:8000/www/tinyemu.html
```

Note TinyEMU browser:

- emulazione disponibile: disco IDE (`smallix.img`), VGA/TTY seriale, PS/2, timer
- passthrough diretto host NIC/USB non disponibile nel browser (limite piattaforma web)

Pkg MJS (bellard/mquickjs):

- path: `pkg/mjs` (include `vendor/mquickjs`)
- comandi:
  - `cargo run --manifest-path pkg/mjs/Cargo.toml -- build-engine`
  - `cargo run --manifest-path pkg/mjs/Cargo.toml -- run <script.mjs>`
  - `cargo run --manifest-path pkg/mjs/Cargo.toml -- check <script.mjs>`
  - `cargo run --manifest-path pkg/mjs/Cargo.toml -- compile <script.mjs> <out.bin>`
  - `cargo run --manifest-path pkg/mjs/Cargo.toml -- to-smallix <input.mjs> <output.mjs>`

Pkg TCC-RS (savannah tinycc vendor):

- path: `pkg/tcc-rs` (tooling host-side)

Installazione dipendenze su Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y qemu-system-x86 qemu-utils grub-pc-bin xorriso mtools
```

Note operative:

- Target kernel: `i686` (Multiboot2 BIOS).
- In ambiente headless, `mkiso.sh` usa fallback `grub-mkstandalone + xorriso` se `grub-mkrescue` fallisce.
- La shell legge input da tastiera PS/2 (`i8042`), quindi per interagire usare la finestra QEMU standard (`./tools/run-qemu.sh`).

## Prerequisiti host

- `rustup` con toolchain nightly e componenti `rust-src`, `llvm-tools-preview`
- `grub-mkrescue`
- `xorriso` (richiesto internamente da grub-mkrescue)
- `qemu-system-x86_64` (o `qemu-system-i386`)
- `qemu-img` (solo per `mkdisk.sh`)
