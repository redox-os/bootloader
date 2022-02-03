ARCH=x86
TARGET=$(ARCH)-unknown-none

export LD=ld -m elf_i386
export OBJCOPY=objcopy
export PARTED=parted
export QEMU=qemu-system-i386
export RUST_TARGET_PATH=$(CURDIR)/targets

all: build/bootloader.bin

clean:
	rm -rf build

build/libbootloader.a: Cargo.lock Cargo.toml src/**
	mkdir -p build
	cargo rustc --lib --target $(TARGET) --release -- -C soft-float -C debuginfo=2 --emit link=$@

build/bootloader.elf: linkers/$(ARCH).ld build/libbootloader.a
	mkdir -p build
	$(LD) --gc-sections -z max-page-size=0x1000 -T $< -o $@ build/libbootloader.a && \
	$(OBJCOPY) --only-keep-debug $@ $@.sym && \
	$(OBJCOPY) --strip-debug $@

build/bootloader.bin: build/bootloader.elf $(ARCH)/**
	mkdir -p build
	nasm -f bin -o $@ -l $@.lst -D STAGE3=$< -i$(ARCH) $(ARCH)/bootloader.asm

build/filesystem:
	mkdir -p build
	rm -f $@.partial
	mkdir $@.partial
	fallocate -l 1MiB $@.partial/kernel
	mv $@.partial $@


build/filesystem.bin: build/filesystem
	mkdir -p build
	rm -f $@.partial
	fallocate -l 255MiB $@.partial
	redoxfs-ar $@.partial $<
	mv $@.partial $@

build/harddrive.bin: build/bootloader.bin build/filesystem.bin
	mkdir -p build
	rm -f $@.partial
	fallocate -l 256MiB $@.partial
	$(PARTED) -s -a minimal $@.partial mklabel msdos
	$(PARTED) -s -a minimal $@.partial mkpart primary 1MiB 100%
	dd if=$< of=$@.partial bs=1 count=446 conv=notrunc
	dd if=$< of=$@.partial bs=512 skip=1 seek=1 conv=notrunc
	dd if=build/filesystem.bin of=$@.partial bs=1MiB seek=1 conv=notrunc
	mv $@.partial $@

qemu: build/harddrive.bin
	$(QEMU) \
		-d cpu_reset \
		-d guest_errors \
		-smp 4 -m 2048 \
		-chardev stdio,id=debug,signal=off,mux=on \
		-serial chardev:debug \
		-mon chardev=debug \
		-machine q35 \
		-net none \
		-enable-kvm \
		-cpu host \
		-drive file=$<,format=raw
