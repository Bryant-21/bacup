Event OnLoad()
    If keypadCode != None
        Game.GetPlayer().SetValue(keypadCode, presetCode as Float)
    EndIf
EndEvent
