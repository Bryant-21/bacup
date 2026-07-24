Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && W05_DontrelleHainesGaveSupplies != None && akSpeakerRef.GetValue(W05_DontrelleHainesGaveSupplies) == 0
        If RadAway != None
            Game.GetPlayer().AddItem(RadAway, 1)
        EndIf
        akSpeakerRef.SetValue(W05_DontrelleHainesGaveSupplies, 1.0)
    EndIf
    If W05_PlayerHasTalkedToDontrelleHaines != None
        Game.GetPlayer().SetValue(W05_PlayerHasTalkedToDontrelleHaines, 1.0)
    EndIf
EndFunction
