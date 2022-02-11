export PARTED?=parted
export QEMU?=qemu-system-aarch64

all: $(BUILD)/bootloader.efi

$(BUILD)/bootloader.efi: Cargo.lock Cargo.toml $(shell find src -type f)
	mkdir -p $(BUILD)
	cargo rustc \
		-Z build-std=core,alloc \
		-Z build-std-features=compiler-builtins-mem \
		--target $(TARGET) \
		--bin bootloader \
		--release \
		-- \
		-C soft-float \
		--emit link=$@

$(BUILD)/bootloader-live.efi: Cargo.lock Cargo.toml $(shell find src -type f)
	mkdir -p $(BUILD)
	cargo rustc \
		-Z build-std=core,alloc \
		-Z build-std-features=compiler-builtins-mem \
		--target $(TARGET) \
		--bin bootloader \
		--release \
		--features live \
		-- \
		-C soft-float \
		--emit link=$@

$(BUILD)/esp.bin: $(BUILD)/bootloader.efi
	mkdir -p $(BUILD)
	rm -f $@.partial
	fallocate -l 64MiB $@.partial
	mkfs.vfat -F 32 $@.partial
	mmd -i $@.partial efi
	mmd -i $@.partial efi/boot
	mcopy -i $@.partial $< ::efi/boot/bootaa64.efi
	mv $@.partial $@

$(BUILD)/harddrive.bin: $(BUILD)/esp.bin $(BUILD)/filesystem.bin
	mkdir -p $(BUILD)
	rm -f $@.partial
	fallocate -l 320MiB $@.partial
	$(PARTED) -s -a minimal $@.partial mklabel gpt
	$(PARTED) -s -a minimal $@.partial mkpart ESP FAT32 1MiB 65MiB
	$(PARTED) -s -a minimal $@.partial mkpart REDOXFS 65MiB 100%
	$(PARTED) -s -a minimal $@.partial toggle 1 boot
	dd if=$(BUILD)/esp.bin of=$@.partial bs=1MiB seek=1 conv=notrunc
	dd if=$(BUILD)/filesystem.bin of=$@.partial bs=1MiB seek=65 conv=notrunc
	mv $@.partial $@

$(BUILD)/firmware.rom:
	wget https://releases.linaro.org/components/kernel/uefi-linaro/latest/release/qemu64/QEMU_EFI.fd -O $@

qemu: $(BUILD)/harddrive.bin $(BUILD)/firmware.rom
	$(QEMU) \
		-d cpu_reset \
		-no-reboot \
		-smp 4 -m 2048 \
		-chardev stdio,id=debug,signal=off,mux=on \
		-serial chardev:debug \
		-mon chardev=debug \
		-machine virt \
		-net none \
		-cpu max \
		-bios $(BUILD)/firmware.rom \
		-drive file=$<,format=raw
