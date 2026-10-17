; Despite the TEST name this rides two live holotapes (FS01_MQ_Warn_MiscRaiderHolotape, pointer 11;
; MTN_TotW_MorgantownAirportHolotape, pointer 4). Playing one advertises its region pointer, as the
; SFM04 pointer book does on read.
Event OnHolotapePlay(ObjectReference akTerminalRef)
    SQ_MasterScript master = SQ_Master as SQ_MasterScript
    If master != None
        master.StartMiscRegionPointer(PointerID, Self)
    EndIf
EndEvent
