# Draft: Waveline Pro Ethernet protocol request

This is a draft for the equipment vendor or authorized service team. It has not been sent.

We are integrating a DRE Waveline Pro with a local research recorder. Please provide the communication protocol specification or SDK matching:

- Software A.3.02
- Kernel board K1-LPC3250-V1.0
- Module board M1-ERTIC-V1.0, type 5A, version V2.0.0

On this unit, the monitor initiates TCP connections from 10.0.0.3 to a central receiver at 10.0.0.2:12345. It sends binary data without application commands from the receiver. A sample has repeating 1,956-byte records beginning `01 02 5b f6 a4 07 00 00`, arriving approximately once per second. The HL7 menu is visible but grayed out. We have not changed the monitor's service settings.

Please confirm:

1. Record framing, byte order, version fields, length semantics, and checksum/integrity rules.
2. Field offsets, types, units, scaling, validity bits, and unavailable-value codes for HR, SpO2, NIBP, temperature, respiration, and gas measurements. For both invasive-pressure channels (IBP1/IBP2, including ART/P1), please identify systolic, diastolic, and mean fields separately from NIBP, channel/site labels, transducer-disconnected and unzeroed states, negative-value encoding, update counters/timestamps, and whether repeated values can be held after a channel stops updating.
3. Waveform channel identifiers, sample encoding, sampling rates, and timing alignment.
4. Whether a read-only receiver needs registration, keepalive, acknowledgments, or any documented handshake.
5. Whether HL7 export is an optional licensed feature on this hardware/firmware, and the supported activation procedure.
6. Availability of a de-identified reference capture with expected decoded output.

No patient data or raw capture is attached to this draft.
