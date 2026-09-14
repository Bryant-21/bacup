Function Fragment_End(ObjectReference akSpeakerRef)
    Actor player = Game.GetPlayer()
    If player != None
        player.SetValue(AV_Beer, 0.0)
        player.SetValue(AV_Liquor, 0.0)
    EndIf
EndFunction
