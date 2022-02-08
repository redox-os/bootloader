TARGET?=x86-unknown-none
BUILD=build/$(TARGET)
export RUST_TARGET_PATH=$(CURDIR)/targets

ifeq ($(TARGET),x86-unknown-none)

export LD=ld -m elf_i386
export OBJCOPY=objcopy
export PARTED=parted
export QEMU=qemu-system-x86_64

all: $(BUILD)/bootloader.bin

else

all:
	$(error target $(TARGET) not supported by bootloader yet)

endif

clean:
	rm -rf build

$(BUILD)/libbootloader.a: Cargo.lock Cargo.toml $(shell find src -type f)
	mkdir -p $(BUILD)
	cargo rustc --lib --target $(TARGET) --release -- -C soft-float -C debuginfo=2 --emit link=$@

$(BUILD)/bootloader.elf: linkers/$(TARGET).ld $(BUILD)/libbootloader.a
	mkdir -p $(BUILD)
	$(LD) --gc-sections -z max-page-size=0x1000 -T $< -o $@ $(BUILD)/libbootloader.a && \
	$(OBJCOPY) --only-keep-debug $@ $@.sym && \
	$(OBJCOPY) --strip-debug $@

$(BUILD)/bootloader.bin: $(BUILD)/bootloader.elf $(shell find asm/$(TARGET) -type f)
	mkdir -p $(BUILD)
	nasm -f bin -o $@ -l $@.lst -D STAGE3=$< -iasm/$(TARGET) asm/$(TARGET)/bootloader.asm

$(BUILD)/filesystem:
	mkdir -p $(BUILD)
	rm -f $@.partial
	mkdir $@.partial
	fallocate -l 1MiB $@.partial/kernel
	mv $@.partial $@


$(BUILD)/filesystem.bin: $(BUILD)/filesystem
	mkdir -p $(BUILD)
	rm -f $@.partial
	fallocate -l 255MiB $@.partial
	redoxfs-ar $@.partial $<
	mv $@.partial $@

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
		-d guest_errors \
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
