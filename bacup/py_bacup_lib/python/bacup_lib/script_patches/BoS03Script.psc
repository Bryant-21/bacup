Function ShowTransponderSignalStrength()
    If !IsRunning() || !IsStageDone(200) || IsStageDone(300) || pCurrentTransponder == None || pBoS03_SignalStrengthMessage == None
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If pPlayer != None && pPlayer.GetActorReference() != None
        playerRef = pPlayer.GetActorReference()
    EndIf
    ObjectReference transponder = pCurrentTransponder.GetReference()
    If playerRef == None || transponder == None
        Return
    EndIf
    ; The server formula is stripped; the radio scene's outer band is 13000 units.
    Float strength = 100.0 * (1.0 - playerRef.GetDistance(transponder) / 13000.0)
    If strength < 0.0
        strength = 0.0
    ElseIf strength > 100.0
        strength = 100.0
    EndIf
    pBoS03_SignalStrengthMessage.Show(strength)
EndFunction
