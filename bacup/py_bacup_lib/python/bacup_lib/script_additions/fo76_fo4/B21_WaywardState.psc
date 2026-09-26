Scriptname B21_WaywardState Hidden

Float Function GameMinutes() Global
    Return (Utility.GetCurrentGameTime() * 1440.0) as Int
EndFunction

Bool Function BadEndingActive(Actor akPlayer, ActorValue akTimestamp, GlobalVariable akCooldown = None) Global
    If akPlayer == None || akPlayer != Game.GetPlayer() || akTimestamp == None
        Return False
    EndIf
    Float timestamp = akPlayer.GetValue(akTimestamp)
    If timestamp <= 0.0
        Return False
    EndIf
    If akCooldown == None
        akCooldown = Game.GetFormFromFile(0x00560192, "SeventySix.esm") as GlobalVariable
    EndIf
    If akCooldown == None
        Return True
    EndIf
    Float nowMinutes = GameMinutes()
    If timestamp > nowMinutes
        akPlayer.SetValue(akTimestamp, nowMinutes)
        Return True
    EndIf
    If nowMinutes - timestamp >= akCooldown.GetValue()
        akPlayer.SetValue(akTimestamp, 0.0)
        Return False
    EndIf
    Return True
EndFunction
