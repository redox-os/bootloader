# Bootloader

Redox OS Bootloader

## Requirements

These software needs to be available on the PATH at build time:

+ [mtools](https://www.gnu.org/software/mtools/)
+ [nasm](https://nasm.us/)
+ [redoxfs-ar](https://gitlab.redox-os.org/redox-os/redoxfs)

## Building

```sh
make TARGET=<triplet> BUILD=build all
```

The `<triplet>` is one of:

| ARCH | Boot Mode | Triplets |
|---|---|---|
| `i686` | BIOS | `x86-unknown-none` |
| `x86_64` | BIOS | `x86-unknown-none` |
| `x86_64` | UEFI | `x86_64-unknown-uefi` |
| `aarch64` | UEFI | `aarch64-unknown-uefi` |
| `riscv64gc` | UEFI | `riscv64gc-unknown-uefi` |

See [mk directory](./mk) for more information of how the build is working.

## Entry points

Please read [Boot Process](https://doc.redox-os.org/book/boot-process.html) in the Redox OS Book for an introductory guide.

In this source code, some interesting files for entry points are:

+ BIOS boot stages: [asm/x86-unknown-none/bootloader.asm](./asm/x86-unknown-none/bootloader.asm)
+ BIOS boot entry: `fn start` at [src/os/bios/mod.rs](./src/os/bios/mod.rs)
+ UEFI boot entry: `fn main` at [src/os/uefi/mod.rs](src/os/uefi/mod.rs)
+ Common boot process: `fn main` at [src/main.rs](src/main.rs)
+ UEFI kernel entry: `fn kernel_entry` in each arch:
  - `x86_64`: [src/os/uefi/arch/x86_64.rs](src/os/uefi/arch/x86_64.rs)
  - `aarch64`: [src/os/uefi/arch/aarch64.rs](src/os/uefi/arch/aarch64.rs)
  - `riscv64gc`: [src/os/uefi/arch/riscv64/mod.rs](src/os/uefi/arch/riscv64/mod.rs)

## Debugging

### QEMU

```sh
make TARGET=<triplet> BUILD=build qemu
```

## How To Contribute

To learn how to contribute to this system component you need to read the following document:

- [CONTRIBUTING.md](https://gitlab.redox-os.org/redox-os/redox/-/blob/master/CONTRIBUTING.md)

## Development

To learn how to do development with this system component inside the Redox build system you need to read the [Build System](https://doc.redox-os.org/book/build-system-reference.html) and [Coding and Building](https://doc.redox-os.org/book/coding-and-building.html) pages.
