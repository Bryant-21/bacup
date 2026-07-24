Function Fragment_End(ObjectReference akSpeakerRef)
    If LL_CapsStash_Standard_Base != None
        Game.GetPlayer().AddItem(LL_CapsStash_Standard_Base, 1)
    EndIf
EndFunction
