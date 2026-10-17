Function Fragment_End(ObjectReference akSpeakerRef)
    ATX_LiteAllyScript ally = GetOwningQuest() as ATX_LiteAllyScript
    If ally
        ally.GiveDesiredItem(8)
    EndIf
EndFunction
