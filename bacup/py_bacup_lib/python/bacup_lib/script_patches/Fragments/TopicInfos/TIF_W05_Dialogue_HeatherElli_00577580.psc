Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && W05_HeatherEllisGaveSupplies != None && akSpeakerRef.GetValue(W05_HeatherEllisGaveSupplies) == 0
        If Stimpak != None
            Game.GetPlayer().AddItem(Stimpak, 1)
        EndIf
        akSpeakerRef.SetValue(W05_HeatherEllisGaveSupplies, 1.0)
    EndIf
    If akSpeakerRef != None && W05_HeatherEllisDislikesPlayer != None
        akSpeakerRef.SetValue(W05_HeatherEllisDislikesPlayer, 1.0)
    EndIf
    If W05_PlayerHasTalkedToHeatherEllis != None
        Game.GetPlayer().SetValue(W05_PlayerHasTalkedToHeatherEllis, 1.0)
    EndIf
EndFunction
