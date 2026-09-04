<div align="center">

# IMDb Tech Manager

[简体中文](../../README.md) | [繁體中文](./README.zh-Hant.md) | [English](./README.en.md) | [Français](./README.fr.md) | [Русский](./README.ru.md) | [日本語](./README.ja.md) | [Español](./README.es.md) | **ไทย**

[![Release](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=release)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Downloads](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=downloads)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)

<img src="../../macos/assets/ITM_logo.png" alt="IMDb Tech Manager" width="220">

เครื่องมือจัดการข้อมูลจำเพาะทางเทคนิคของภาพยนตร์และเมทาดาทาของคลังสื่อ

</div>

## เกี่ยวกับโครงการ

IMDb Tech Manager (ITM) ดึงและจัดโครงสร้าง IMDb Technical Specifications เพื่อบันทึกข้อมูลกล้อง เลนส์ รูปแบบการถ่ายทำ เสียง อัตราส่วนภาพ และกระบวนการผลิตลงใน NFO อย่างปลอดภัย นอกจากนี้ยังมี Inspector การแสดงตัวอย่างก่อนเขียน การย้อนกลับ งานแบบกลุ่ม และการจัดการแท็กทางเทคนิคด้วยกฎภายในหรือ AI

ITM สามารถทำงานร่วมกับ [Tech Card Manager (TCM)](https://github.com/Eric-Hou1997/Tech-Card-Manager):

```text
IMDb → ITM → NFO / Technical Specifications → TCM → แสดงผลใน Emby
```

## เวอร์ชันและแพลตฟอร์ม

- เวอร์ชันเสถียร: [`v4.1.0`](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/tag/v4.1.0)
- แพลตฟอร์ม: macOS 12 ขึ้นไป, Apple Silicon (arm64)
- ไฟล์เผยแพร่: `ITM-v4.1.0-MacOS-AArch64-APP.zip`
- ชื่อแอปภายใน ZIP คงที่เป็น `IMDb Tech Manager.app`
- Release มี SHA-256 ลายเซ็น OTA Ed25519 คำแนะนำ และบันทึกการเปลี่ยนแปลง

## ความสามารถหลัก

- ดึง แคช และจัดโครงสร้าง IMDb Technical Specifications
- เขียน NFO อย่างปลอดภัยเฉพาะใน `<technicalspecs>`
- Inspector, ตัวอย่างก่อนเขียน, ตรวจสอบแฮชต้นฉบับ, สำรองข้อมูล และย้อนกลับ
- แยกความเป็นเจ้าของแท็ก External, Generated และ Manual อย่างชัดเจน
- ค้นหา กรอง เลือก และทำงานแบบกลุ่มแยกกันสำหรับภาพยนตร์และรายการทีวี
- เปลี่ยนผู้ให้บริการ AI แบบ OpenAI-compatible และ Anthropic ได้
- นับคำขอ HTTP, Token, แคช และค่าใช้จ่ายจริง
- OTA ที่แยกข้อผิดพลาดพร็อกซี ขีดจำกัด GitHub ไฟล์หาย การดาวน์โหลด และลายเซ็น

## ภาษา

ภาษาจีนตัวย่อ ภาษาจีนตัวเต็ม และ English (United States) รวมอยู่ในแอป ส่วนภาษาฝรั่งเศส รัสเซีย ญี่ปุ่น สเปน และไทยเผยแพร่เป็นแพ็กภาษาแยกใน Release `v4.1.0` และจะโหลดหลังจากดาวน์โหลดและตรวจสอบแล้วเท่านั้น

Web UI, Go Core, Python Engine และเมนู macOS ใช้สถานะภาษาเดียวกัน ภาษาของบันทึกและคำอธิบายการตรวจทานจะถูกกำหนดเมื่อเริ่มงาน การเปลี่ยนภาษา UI จะไม่เขียนทับบันทึกเดิม แคช NFO ข้อมูลความเป็นเจ้าของ หรือ prompt ของผู้ใช้

## ขอบเขตความปลอดภัย

- Spec Agent แก้ไขได้เฉพาะ `<technicalspecs>` และไม่แก้ `<tag>` ระดับราก
- ระบบอัตโนมัติแก้เฉพาะ Generated Tech Tags ที่พิสูจน์ความเป็นเจ้าของได้
- แท็ก External, Manual, TMM และแท็กของแอปอื่นจะถูกเก็บไว้
- XML, UTF-8 BOM, รูปแบบบรรทัด, สิทธิ์ไฟล์, ข้อมูลสำรอง และการแทนที่แบบอะตอมจะถูกเก็บหรือตรวจสอบ
- หากเส้นทางไม่ชัดเจน ความเป็นเจ้าของขัดแย้ง หรือไฟล์เปลี่ยนหลังแสดงตัวอย่าง ระบบจะหยุดอย่างปลอดภัย

## ภาพหน้าจอ

![การจัดการข้อมูล](../images/data-management.png)

![การจัดการแท็ก](../images/tag-management.png)

## Roadmap

เสร็จแล้ว: การเผยแพร่ Apple Silicon, ความปลอดภัยและความเป็นเจ้าของ NFO, แท็ก Local/AI, การจัดการงานและการใช้งาน, UI ในตัวสามภาษา, ภาษาแบบดาวน์โหลดห้าภาษา, การอัปเดตผูกกับเวอร์ชัน และด่านทดสอบถดถอย

กำลังดำเนินการ: การทำ Technical Specifications ให้เป็นมาตรฐาน, การทดสอบกับคลังจริง, ความเข้ากันได้ของผู้ให้บริการ AI, การกู้คืนข้อผิดพลาด, Developer ID และ notarization รวมถึงแพลตฟอร์มและเซิร์ฟเวอร์สื่ออื่น

## การพัฒนาและสัญญาอนุญาต

โปรดอ่าน [`AGENTS.md`](../../AGENTS.md) ก่อนพัฒนา การออกแบบแพ็กภาษาอยู่ใน [`docs/language-packs.md`](../language-packs.md)

โครงการนี้ใช้ [Apache License 2.0](../../LICENSE) เครื่องหมายการค้า IMDb, Emby และอื่น ๆ เป็นของเจ้าของแต่ละราย โครงการนี้ไม่มีความเกี่ยวข้อง การอนุญาต หรือการรับรองจาก IMDb.com, Inc. หรือ Emby LLC
