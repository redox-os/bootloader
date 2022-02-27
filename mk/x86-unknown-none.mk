export LD?=ld
export OBJCOPY?=objcopy
export PARTED?=parted
export QEMU?=qemu-system-x86_64

all: $(BUILD)/bootloader.bin

$(BUILD)/libbootloader.a: Cargo.lock Cargo.toml $(shell find src -type f)
	mkdir -p $(BUILD)
	env RUSTFLAGS="-C soft-float" \
	cargo rustc \
		-Z build-std=core,alloc \
		-Z build-std-features=compiler-builtins-mem \
		--target $(TARGET) \
		--lib \
		--release \
		-- \
		--emit link=$@

$(BUILD)/bootloader.elf: linkers/$(TARGET).ld $(BUILD)/libbootloader.a
	mkdir -p $(BUILD)
	$(LD) -m elf_i386 --gc-sections -z max-page-size=0x1000 -T $< -o $@ $(BUILD)/libbootloader.a && \
	$(OBJCOPY) --only-keep-debug $@ $@.sym && \
	$(OBJCOPY) --strip-debug $@

$(BUILD)/bootloader.bin: $(BUILD)/bootloader.elf $(shell find asm/$(TARGET) -type f)
	mkdir -p $(BUILD)
	nasm -f bin -o $@ -l $@.lst -D STAGE3=$< -iasm/$(TARGET) asm/$(TARGET)/bootloader.asm

$(BUILD)/libbootloader-live.a: Cargo.lock Cargo.toml $(shell find src -type f)
	mkdir -p $(BUILD)
	env RUSTFLAGS="-C soft-float" \
	cargo rustc \
		-Z build-std=core,alloc \
		-Z build-std-features=compiler-builtins-mem \
		--target $(TARGET) \
		--lib \
		--release \
		--features live \
		-- \
		--emit link=$@

$(BUILD)/bootloader-live.elf: linkers/$(TARGET).ld $(BUILD)/libbootloader-live.a
	mkdir -p $(BUILD)
	$(LD) -m elf_i386 --gc-sections -z max-page-size=0x1000 -T $< -o $@ $(BUILD)/libbootloader-live.a && \
	$(OBJCOPY) --only-keep-debug $@ $@.sym && \
	$(OBJCOPY) --strip-debug $@

$(BUILD)/bootloader-live.bin: $(BUILD)/bootloader-live.elf $(shell find asm/$(TARGET) -type f)
	mkdir -p $(BUILD)
	nasm -f bin -o $@ -l $@.lst -D STAGE3=$< -iasm/$(TARGET) asm/$(TARGET)/bootloader.asm

$(BUILD)/harddrive.bin: $(BUILD)/bootloader.bin $(BUILD)/filesystem.bin
	mkdir -p $(BUILD)
	rm -f $@.partial
	fallocate -l 256MiB $@.partial
	$(PARTED) -s -a minimal $@.partial mklabel msdos
	$(PARTED) -s -a minimal $@.partial mkpart primary 1MiB 100%
	dd if=$< of=$@.partial bs=1 count=446 conv=notrunc
	dd if=$< of=$@.partial bs=512 skip=1 seek=1 conv=notrunc
	dd if=$(BUILD)/filesystem.bin of=$@.partial bs=1MiB seek=1 conv=notrunc
	mv $@.partial $@

qemu: $(BUILD)/harddrive.bin
	$(QEMU) \
		-d cpu_reset \
		-no-reboot \
		-smp 4 -m 2048 \
		-chardev stdio,id=debug,signal=off,mux=on \
		-serial chardev:debug \
		-mon chardev=debug \
		-machine q35 \
		-net none \
		-enable-kvm \
		-cpu host \
		-drive file=$<,format=raw
