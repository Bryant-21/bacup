Function Fragment_End(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If Alias_currentPlayer != None && playerRef != None
        Alias_currentPlayer.ForceRefTo(playerRef)
    EndIf
    If W05_MQR_204P_WarningMSG != None
        W05_MQR_204P_WarningMSG.Show()
    EndIf
EndFunction
