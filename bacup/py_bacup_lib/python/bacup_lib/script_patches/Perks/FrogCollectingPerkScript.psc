Event OnEntryRun(Int auiEntryID, ObjectReference akTarget, Actor akOwner)
    If auiEntryID == 0
        akOwner.AddItem(FrogItem, 1)
    EndIf
EndEvent
