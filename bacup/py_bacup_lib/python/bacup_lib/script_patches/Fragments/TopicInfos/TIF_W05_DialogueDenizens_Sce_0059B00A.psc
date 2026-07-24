Function Fragment_End(ObjectReference akSpeakerRef)
    DenizenDialogueScript ownerQuest = GetOwningQuest() as DenizenDialogueScript
    If ownerQuest != None && RepMod != None
        ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()
    EndIf
EndFunction
