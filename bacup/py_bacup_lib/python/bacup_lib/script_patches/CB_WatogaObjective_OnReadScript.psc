; Reading a Watoga lead note starts the Watoga misc region pointer (CB_Master's CB_Watoga_GQ_MiscRegionPointer_ID).
Event OnRead()
    CB_MasterScript master = CB_Master as CB_MasterScript
    If master == None || master.SQ_Master == None || master.CB_Watoga_GQ_MiscRegionPointer_ID == None
        Return
    EndIf
    (master.SQ_Master as SQ_MasterScript).StartMiscRegionPointer(master.CB_Watoga_GQ_MiscRegionPointer_ID.GetValueInt(), Self)
EndEvent
