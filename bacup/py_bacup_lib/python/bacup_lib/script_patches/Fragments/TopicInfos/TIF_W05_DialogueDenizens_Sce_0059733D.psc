Function Fragment_End(ObjectReference akSpeakerRef)
    If RemoveRef != None
        Game.GetPlayer().RemoveItem(RemoveRef, 1)
    EndIf
    DenizenDialogueScript ownerQuest = GetOwningQuest() as DenizenDialogueScript
    If ownerQuest != None && RepMod != None
        ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()
    EndIf
EndFunction
