# MiniMidi Controller

MiniMidi is a very small, low cost, simple Midi controller that works over Bluetooth.
It is part of the Tonex One Controller project, but could be used on any other BT Central device.

# Table of Contents
 1. [Key Features](#key_features)
 2. [Hardware Platforms and Wiring Diagrams](#hardware_platforms)
 3. [Uploading/Programming Firmware Releases](#firmware_uploading)
 4. [Configuration and Settings](#config)
 5. [Acknowledgements](#acknowledgements)
 6. [Firmware Release Notes](#release_notes)
 7. [License](#license)

## ⭐ Key Features <a name="key_features"></a>
- Low cost Bluetooth Midi control for sending program change commands
- Option of 2 button mode doing next/previous
- Option of 4 button mode, doing bank selection up via 1+2 and bank select down via 3+4
- Runs on most ESP32-S3 boards. Does not require PSRAM

## ⭐ Hardware Platforms <a name="hardware_platforms"></a>
Tested on the Waveshare ESP32-S3 Zero board:
[https://www.waveshare.com/product/esp32-s3-zero.htm](https://www.waveshare.com/product/esp32-s3-zero.htm)

**Ensure its the ESP32-S3FH4R2 with USB-C port. There are some similar boards with no USB-C or slightly different processor**
![footswitches_waveshare_zero](https://github.com/user-attachments/assets/205eead9-6f3b-4c88-80c3-78308731c285)
<br>
Example two-button cheap controller
![image](https://github.com/user-attachments/assets/403126cc-d476-4378-b435-549592508654)

## ⭐ Uploading/Programming Firmware Releases <a name="firmware_uploading"></a> 
Refer here for the procedure to upload firmware to the board:
[https://github.com/Builty/TonexOneController/blob/main/FirmwareUploading.md ](https://github.com/Builty/TonexOneController/blob/main/FirmwareUploading.md)
 
## ⭐ Configuration and Settings <a name="config"></a>  
Coming soon
 
## 🙏 Acknowledgements <a name="acknowledgements"></a>
Blemidi library obtained from https://github.com/midibox/esp32-idf-blemidi

## ⭐ Firmware Release Notes <a name="release_notes"></a>  
Coming soon

## ©️ License <a name="license"></a>
The MiniMidi Controller is under the Apache 2.0 license. It is free for both research and commercial use cases.
<br>However, if you are stealing this work and commercialising it, you are a bad person and you should feel bad.
<br>As per the terms of the Apache license, you are also required to provide "attribution" if you use any parts of the project (link to this project from your project.)
