# Waveline Pro protocol research and photo validation

Research date: October 21, 2026. Captured unit: firmware A.3.02. This is a record of evidence and rejected hypotheses, not a vendor protocol specification.

## Findings

No public byte-level Ethernet definition was found in the Waveline Pro, Avante/DRE, Infinium OMNI III, or OMNIVIEW material searched. Manufacturer descriptions advertise communication support, but that is not a packet specification. The raw captures still do **not establish a gas or CO₂ mapping or an exact ECG calibration**. Missing data must remain empty; the photo values below are reference observations, never replacement measurements.

A useful device-family lead is the [Waveline Pro model 8858 listing](https://accessgudid.nlm.nih.gov/devices/00815871021937), which references clearance K101052. The [FDA clearance document](https://www.accessdata.fda.gov/cdrh_docs/pdf10/K101052.pdf) identifies Infinium's OMNI III. This links a listed model, not necessarily every Waveline Pro firmware revision or network format. [Infinium's OMNI III product description](https://infiniummedical.com/omni-iii-temp/) advertises Ethernet/RS-232 communication and central monitoring without publishing the binary layout.

The [OMNI III manufacturer's manual, Rev. 2, hosted by Tritec](https://www.tritecmonitors.com/_files/ugd/e67dee_269a00191ef24b1b849c1e61e151e039.pdf) provides useful display context:

- PDF page 118 specifies a 15-inch, 1024×768 screen.
- PDF pages 33–34 describe adjustable ECG gain/sweep and cascade as one waveform continuing onto a second row.
- PDF page 20 distinguishes RESP, EtCO₂, and anesthetic-gas channels/modules.
- PDF page 90 describes the Dräger gas display; other sections cover alternative gas modules.
- Its Ethernet interface section describes connection purposes, not frame offsets, scale factors, sample clocks, or optional-gas subscriptions.

These related-model facts do not authorize assigning its internal sensor sampling rates or serial-port settings to the recorded Ethernet stream. They also do not establish the captured device's exact firmware layout.

## Photo reference table

Date and zone were confirmed by the user: **October 20, 2026, America/New_York (EDT, UTC−04:00)**. Times below are supplied photo times, not the monitor's incorrect clock. Images were manually reinspected during this research.

| Display | 08:52:51 — IMG_1507 | 10:38:42 — IMG_1508 | 11:57:32 — IMG_1511 |
|---|---:|---:|---:|
| Monitor clock, for identification only | 04:47 AM | 06:33 AM | 07:52 AM |
| ECG HR, bpm | Lead Off / dashes | 88 | 110 |
| ECG channel / filter | II / Surgery | II / Surgery | II / Surgery |
| ECG gain shown | ×1.0 | ×2.0 | ×2.0 |
| ECG sweep shown, mm/s | 50.0 | 25.0 | 25.0 |
| ECG calibration marker | 1 mV | 1 mV | 1 mV |
| Cuff SYS / DIA, mmHg | 103 / not visible | 80 / 52 | 148 / 115 |
| Small cuff-side number, meaning unresolved | 8 | 39 | 43 |
| Cuff mode/status | Small, 02:00 Auto; Auto measuring | same | same |
| ET CO₂, mmHg | 24 | 43 | 39 |
| FI CO₂, mmHg | 0 | 1 | 0 |
| Gas RR, breaths/min | 18 | 12 | 7 |
| ET O₂, % | 48.7 | 86.3 | 87.6 |
| FI O₂, % | 87.1 | 91.8 | 93.3 |
| ET / FI N₂O, % | 0.0 / 0.0 | 0.0 / 0.0 | 0.0 / 0.0 |
| ET ISO, % | 1.09 | 2.35 | 1.46 |
| FI ISO, % | 2.55 | 2.54 | 1.25 |
| MAC | 0.95 | 2.04 | 1.27 |
| CO₂ / pleth sweep, mm/s | 12.5 / 12.5 | 12.5 / 12.5 | 12.5 / 12.5 |
| SpO₂, % | not legible | 95 | Sensor Off / dashes |
| Pulse rate, bpm | 99 | 61 | Sensor Off / dashes |
| Perfusion index, % | 2.55 | 1.12 | dashes |
| T1, °F | not legible | 99.3 | 98.6 |
| T2 | dashes | dashes | dashes |
| P1 SYS / DIA / MAP, mmHg | Sensor Off / dashes | 178 / 178 / 178 | 127 / 87 / 100 |
| P2 | Sensor Off / dashes | Sensor Off / dashes | Sensor Off / dashes |
| ART / PA axis labels, mmHg | 0 and 150 | 0 and 150 | 0 and 150 |
| ART / PA sweep, mm/s | 25.0 / 25.0 | 25.0 / 25.0 | 25.0 / 25.0 |

The earliest photo belongs to the preceding connection-test session, not the later session named pig 6. Its legacy journal timestamps are batched, so it is a weaker timing reference. The two later photos agree with already identified HR, cuff SYS/DIA, T1 and P1 fields. This establishes useful alignment without aligning on unknown gas values.

At 11:57:32 the latest preceding transport receipt is **15:57:31.676649Z**. At 10:38:42 it is **14:38:41.029764Z**; the next receipt at **14:38:42.032086Z** also matches the photographed main numerics. A photo specified to the second is not millisecond acquisition synchronization.

## Unit conversions are part of the search

The Celsius/Fahrenheit result is the model: identify an encoding and use **one consistent conversion across independent photos**, while preserving the raw value. For example, T1 37.4°C converts to 99.32°F, displayed as 99.3°F.

This pass tests decimal and binary fixed-point storage, little/big endian integer and float formats (including half precision), packed BCD, and integer bitfields. Unit-aware tests include oxygen/agent fractions versus percent, ppm versus percent, CO₂ kPa/Pa/atmospheres versus mmHg, and gas partial pressure versus volume percent. Pressure-dependent tests use shared candidate ambient pressures from 650 to 850 mmHg in 5-mmHg steps; these are search assumptions, not measured ambient pressures. RR in cycles/second versus per-minute is also checked. These searches are finite and do not exclude every possible proprietary encoding, varying pressure correction, compression, or multiplexing scheme.

MAC may be calculated rather than independently transmitted: **ET ISO / 1.15**, rounded to two decimals, gives **0.95, 2.04, 1.27** for the three photos. This arithmetic relationship is a hypothesis, not validation of the monitor's MAC formula, adjustment settings, or intended species. No ISO mapping means it cannot yet produce a captured MAC value.

Search hits are retained for inspection rather than automatically promoted. A countdown can happen to equal almost any small integer in a wide window. Bit fragments of temperature and pressure can likewise resemble CO₂ or RR. A candidate needs independent evidence of identity, timing, and status—not just one numerical match. Constant zero N₂O values cannot locate a field among the many zero bytes.

## What the capture contains

The copied sessions contain **9,763 complete 1,956-byte records**, including **8,025 pig 6 records**. Reassembly accounts for all recorded received payload bytes, with zero discarded or trailing bytes. This rules out another unparsed message hiding between these recorded frames; it does **not** prove all traffic from the monitor was captured. The application recorded accepted TCP payloads, not every Ethernet interface or port.

Only these byte positions change across the copied captures:

- 14; 20–21; 28; 30; 32; 36–47; 64; 66; 68–71: already investigated numerics.
- 88–89, 349, 417, 661: channel-state candidates.
- 180–251, 356–415, 424–551, 668–795: ECG, pleth, RESP-candidate and P1 waveform blocks.
- 1500: unknown configuration byte, identical to byte 88 in this capture; 1516: countdown candidate.

The remaining bytes, including the tail after 1516, are constant across these records. A constant region cannot encode the changing O₂/ISO values through a fixed unit conversion alone. A proprietary shared encoding inside changing regions is still logically possible, but no supported mapping has been found.

The original `testocnnect.pcapng` was also read without modification. Its 34 packets involving 10.0.0.3 contain ARP and failed/reset TCP connection attempts to port 12345, with **zero monitor TCP application payload**. It cannot supply the missing gas stream.

The expanded ±30-second paired search found no O₂, ISO or MAC candidates under its tested encodings. Apparent CO₂/RR hits overlap already known temperature, HR, P1 or countdown fields. An additional three-photo exact-pattern scan includes all bytes, even waveform regions, and finds no consistent O₂/ISO/MAC offsets for the tested integer/float representations. The three-photo unit-aware search also produced zero matching gas/RR/MAC candidates under the tested conversions. Its earliest photo window has weaker timestamp alignment, so this is not proof of absence. Unit-aware search results and every retained hit are saved separately with the analysis scripts.

The working explanation is that the current recorded message/configuration does not expose the optional gas measurements in an identified form. This is stronger than “the UI has no label,” but weaker than proof that the hardware can never transmit them.

## Waveform scaling evidence

### P1 / ART

The little-endian u32 at **664** is consistently **150**, immediately preceding the P1 bytes **668–795**. The analogous P2 word at **800** is also 150. Both match the photographed upper axis labels. This is a plausible display-range field, not a confirmed schema.

A candidate conversion is `pressure ≈ raw_byte × 150 / 255`. Across 2,342 unclipped, varying P1 records selected by explicit sanity conditions, raw extrema/mean correlate with recorded SYS/DIA/MAP at **0.965 / 0.951 / 0.975**. Mean absolute differences under that conversion are approximately **5.51 / 5.55 / 5.33 mmHg**. There are 95th-percentile errors around **14.0 / 15.35 / 12.84 mmHg**. Those differences prohibit calling this an exact calibration.

In 1,121 relatively stable windows, the waveform mean versus recorded MAP correlation improves to about **0.994**, but error remains. Monitor averaging, phase, filtering, block timing and true scaling remain competing explanations. Fitting the same recorded numeric values is not independent validation.

At 10:38 the P1 numeric display is 178/178/178 while all waveform bytes are 255: consistent with a trace clipped above a 150-mmHg display range. At 11:57 the latest block has min/max/mean **161 / 212 / 179.117**, which the candidate converts to **94.71 / 124.71 / 105.36 mmHg**, versus recorded **127/87/100**. A one-second block's extrema and mean need not equal the device's processed numerics.

### ECG and time axis

The active ECG candidate contains 72 bytes per frame. In the minute preceding 11:57:32, concatenating regularly received frames produces a dominant spectral component near **111.7 cycles/minute**, consistent with the median recorded HR of **111**. This supports channel identity and roughly one second per block; it does not establish a hardware sample clock or counts-per-mV conversion.

The photo's ×2 gain and 1-mV marker describe the physical display. They cannot alone reveal whether Ethernet samples are before or after that gain. The current app's visual zoom must remain labeled as visual zoom, not calibrated mV gain. Different sweep speeds also mean ECG/P1 and CO₂/pleth should not automatically share the same horizontal history duration in an eventual calibrated display.

Short block overlaps and repeated blocks exist; there is no single fixed overlap length that explains every channel. A P1 spectrum gives a somewhat different pulse frequency from ECG under the receipt-time assumption. These observations leave block duration, overlap and sample-clock details unresolved. No exact per-sample timestamp or clinical unit is assigned.

The 424–551 block does not validate as capnography: its behavior is inconsistent across the two later photo targets. For example, around 11:57 its dominant variations are near 61 and 122 cycles/minute rather than the displayed gas RR of 7. Respiratory modulation elsewhere is not itself a CO₂ concentration waveform.

## Reproducibility and next discriminating evidence

Local, Git-ignored artifacts: `capture-analysis/pig6-photo-correlation-2026-10-21/protocol-research/`. Scripts read the existing `working-copies/`; results include byte variation, all conversion-search hits, pressure regressions, spectral probes, photo references, packet summary and preservation checks. No source capture/export is rewritten, and no captured value is replaced with a photo transcription.

To resolve the remaining fields, the most useful next evidence is the **Waveline Pro A.3.02 Ethernet/central-station interface specification**: 1,956-byte `01 02 5b f6` frames on TCP port 12345; optional Dräger gas/AG export enablement or subscription messages; CO₂/O₂/N₂O/agent field units and validity codes; waveform counts-to-mV/mmHg conversion; sample rates, buffer overlap and timestamps. Infinium/OMNIVIEW is a documented lead, not a substitute protocol. No vendor message has been sent.

A new passive full-interface capture while the monitor is connected to its supported central station could distinguish omitted gas fields from a separate connection or requested stream. A simulator with known ECG amplitude and pressure levels would resolve waveform calibration independently. Those experiments require the device to be available; the current files cannot be made more informative by changing their original contents.

Preservation verification: **64 original files and 64 replay-copy files all match their previously recorded SHA-256 hashes**. Application decoding and stored CSV schemas were not changed in this research pass.


## Continued investigation: image measurements and alternate encodings

A further pass extracted ECG/CO₂ trace positions and ECG calibration-marker heights from the actual photos, tested mixed-word floats, 24-bit and biased integers, strided-byte fields, and waveform bit planes, and compared every archived raw stream with its session counterpart.

The measured photo amplitudes suggest roughly **130–145 ECG counts per mV**, making **128 counts/mV a testable hypothesis**, not verified calibration. The exploratory horizontal fits favor about **8.2 seconds per ECG row** but do not identify an exact time offset. All **740** records with P1 status 1 have the photo-supported 351/351/351 disconnected signature. Byte **1500** was corrected from a cuff-state guess to an unknown configuration byte: it mirrors byte 88 exactly and is confounded with several changes.

No gas mapping survived these additional checks. The alleged CO₂ block also fails direct photo-shape comparison at a consistent sweep duration. All five raw streams in Archive.zip match the session copies. Detailed methods, raw fit results and mismatch plots are in [the continued investigation](../capture-analysis/pig6-photo-correlation-2026-10-21/waveform-forensics/README.md) (local, Git-ignored). These exploratory fits have not replaced app calibration or captured values.

## Saved-file follow-up: pressure arithmetic and cuff timing

P1 MAP matches truncation of `(SYS + 2*DIA)/3` in **9,727 of 9,763 records**, with only +1 residuals in the remaining 36 near-zero records. All **162 extreme P1 records** follow the formula too; masking a high bit in SYS alone is not supported. Every one of the **4,067 nonzero cuff MAP values** matches the same formula rounded to nearest integer. These are observed relationships, not instructions to manufacture missing values.

At both later photo targets the cuff MAP field is zero. It clears while prior SYS/DIA remain, then updates with a new triple after the photographed “Auto measuring” state. This supports a cuff-cycle explanation for the small photographed numbers 39 and 43, whose identity remains unresolved; those numbers must not be substituted for transmitted MAP.

Holding out complete recording segments also rejects a simple fitted pressure-scale correction: the largest held-out segment has **12.79** mean absolute error under the fit versus **5.82** under the existing exploratory `150/255` candidate. Neither is verified calibration. MAP's arithmetic dependence on SYS/DIA weakens its usefulness as an independent waveform-average target.

Scripts, full timelines and a separate 9,763-row audit CSV are in [pressure forensics](../capture-analysis/pig6-photo-correlation-2026-10-21/pressure-forensics/README.md) (local, Git-ignored). All **64 originals and 64 replay copies** were hashed again and remain unchanged. No decoder, original CSV, waveform calibration or gas mapping was changed in this pass.
