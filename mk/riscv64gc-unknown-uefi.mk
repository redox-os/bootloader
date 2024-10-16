LD=riscv64-unknown-redox-ld
OBJCOPY=riscv64-unknown-redox-objcopy
SCRIPT=$(SOURCE)/linkers/riscv64-unknown-uefi.ld
PARTED?=parted
QEMU?=qemu-system-riscv64

all: $(BUILD)/bootloader.efi

$(BUILD)/%.efi: $(BUILD)/%.efi.elf $(BUILD)/%.efi.sym
	$(OBJCOPY) -j .text -j .data -j .rdata -j .rela -j .reloc --target pei-riscv64-little \
	           --file-alignment 512 --section-alignment 4096 --subsystem 10 "$<" "$@"

.PRECIOUS: $(BUILD)/%.efi.sym
$(BUILD)/%.efi.sym: $(BUILD)/%.efi.elf
	$(OBJCOPY) --only-keep-debug "$<" "$@"

$(BUILD)/%.efi.elf: $(BUILD)/%.a $(SCRIPT)
	$(LD) --gc-sections -z max-page-size=0x1000 --warn-common --no-undefined -z nocombreloc -shared \
	      --fatal-warnings -Bsymbolic --entry coff_start -T "$(SCRIPT)" -o "$@" "$<"

$(BUILD)/bootloader.a: $(SOURCE)/Cargo.toml $(SOURCE)/Cargo.lock $(shell find $(SOURCE)/src -type f)
	mkdir -p "$(BUILD)"
	env RUSTFLAGS="-C soft-float" \
	cargo rustc \
		--manifest-path="$<" \
		-Z build-std=core,alloc \
		-Z build-std-features=compiler-builtins-mem \
		--target $(TARGET) \
		--lib \
		--release \
		-- \
		--emit link=$@

$(BUILD)/bootloader-live.a: $(SOURCE)/Cargo.toml $(SOURCE)/Cargo.lock $(shell find $(SOURCE)/src -type f)
	mkdir -p "$(BUILD)"
	env RUSTFLAGS="-C soft-float" \
	cargo rustc \
		--manifest-path="$<" \
		-Z build-std=core,alloc \
		-Z build-std-features=compiler-builtins-mem \
		--target $(TARGET) \
		--lib \
		--release \
		--features live \
		-- \
		--emit link=$@


$(BUILD)/esp.bin: $(BUILD)/bootloader.efi
	rm -f $@.partial
	fallocate -l 64MiB $@.partial
	mkfs.vfat -F 32 $@.partial
	mmd -i $@.partial EFI
	mmd -i $@.partial EFI/BOOT
	mcopy -i $@.partial $< ::EFI/BOOT/BOOTRISCV64.EFI
	mv $@.partial $@

$(BUILD)/harddrive.bin: $(BUILD)/esp.bin $(BUILD)/filesystem.bin
	rm -f $@.partial
	fallocate -l 320MiB $@.partial
	$(PARTED) -s -a minimal $@.partial mklabel gpt
	$(PARTED) -s -a minimal $@.partial mkpart ESP FAT32 1MiB 65MiB
	$(PARTED) -s -a minimal $@.partial mkpart REDOXFS 65MiB 100%
	$(PARTED) -s -a minimal $@.partial toggle 1 boot
	dd if=$(BUILD)/esp.bin of=$@.partial bs=1MiB seek=1 conv=notrunc
	dd if=$(BUILD)/filesystem.bin of=$@.partial bs=1MiB seek=65 conv=notrunc
	mv $@.partial $@

$(BUILD)/fw_vars.img: /usr/share/qemu-efi-riscv64/RISCV_VIRT_VARS.fd
	cp "$<" "$@"

$(BUILD)/firmware.rom: /usr/share/qemu-efi-riscv64/RISCV_VIRT_CODE.fd
	cp "$<" "$@"

qemu-acpi: $(BUILD)/harddrive.bin $(BUILD)/firmware.rom $(BUILD)/fw_vars.img
	$(QEMU) \
	    -M virt \
		-d cpu_reset \
		-no-reboot \
		-smp 4 -m 2048 \
		-chardev stdio,id=debug,signal=off,mux=on \
		-serial chardev:debug \
		-mon chardev=debug \
		-device virtio-gpu-pci \
		-machine virt \
		-net none \
		-cpu max \
		-drive if=pflash,format=raw,unit=0,file=$(BUILD)/firmware.rom,readonly=on \
		-drive if=pflash,format=raw,unit=1,file=$(BUILD)/fw_vars.img \
		-drive file=$(BUILD)/harddrive.bin,format=raw,if=virtio


qemu-dtb: $(BUILD)/harddrive.bin $(BUILD)/firmware.rom $(BUILD)/fw_vars.img
	$(QEMU) \
	    -M virt,acpi=off \
		-d cpu_reset \
		-no-reboot \
		-smp 4 -m 2048 \
		-chardev stdio,id=debug,signal=off,mux=on \
		-serial chardev:debug \
		-mon chardev=debug \
		-device virtio-gpu-pci \
		-machine virt \
		-net none \
		-cpu max \
		-drive if=pflash,format=raw,unit=0,file=$(BUILD)/firmware.rom,readonly=on \
		-drive if=pflash,format=raw,unit=1,file=$(BUILD)/fw_vars.img \
		-drive file=$(BUILD)/harddrive.bin,format=raw,if=virtio -s
