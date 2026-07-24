Function Fragment_End(ObjectReference akSpeakerRef)
    If RemoveDrug != None
        Game.GetPlayer().RemoveItem(RemoveDrug, 1)
    EndIf
    DenizenDialogueScript ownerQuest = GetOwningQuest() as DenizenDialogueScript
    If ownerQuest != None && RepMod != None
        ownerQuest.RepConvoOutcomeDV = ownerQuest.RepConvoOutcomeDV + RepMod.GetValueInt()
    EndIf
    If Addictol != None
        Game.GetPlayer().AddItem(Addictol, 1)
    EndIf
EndFunction
