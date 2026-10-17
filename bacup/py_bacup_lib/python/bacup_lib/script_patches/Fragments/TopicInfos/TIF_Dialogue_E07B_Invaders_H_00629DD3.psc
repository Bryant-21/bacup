Function Fragment_End(ObjectReference akSpeakerRef)
    Actor player = Game.GetPlayer()
    ObjectReference spray = SprayRef
    If spray == None && Alias_BuffSpray
        spray = Alias_BuffSpray.GetReference()
    EndIf
    If spray
        spray.EnableNoWait()
    EndIf
    If E07B_Invaders_HomerBuff
        E07B_Invaders_HomerBuff.Cast(player, player)
    EndIf
    If E07B_Invaders_Message_HomerBuff
        E07B_Invaders_Message_HomerBuff.Show()
    EndIf
    If spray
        Utility.Wait(10.0)
        spray.DisableNoWait()
    EndIf
EndFunction
