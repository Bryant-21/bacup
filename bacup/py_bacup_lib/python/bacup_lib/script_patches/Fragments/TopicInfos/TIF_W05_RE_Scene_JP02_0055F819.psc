Function Fragment_End(ObjectReference akSpeakerRef)
    Actor player = Game.GetPlayer()
    Actor settler = akSpeakerRef as Actor
    If player != None
        If SettlerAllies != None
            SettlerAllies.RemoveRef(player)
        EndIf
        If settler != None
            settler.StartCombat(player)
        EndIf
    EndIf
EndFunction
