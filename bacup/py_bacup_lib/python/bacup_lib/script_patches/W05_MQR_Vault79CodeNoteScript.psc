Event OnRead()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || W05_MQ00_CodeAV == None
        Return
    EndIf

    Int vault79Code = playerRef.GetValue(W05_MQ00_CodeAV) as Int
    If vault79Code >= 100000 && vault79Code <= 999999
        Debug.Notification("Vault 79 keypad code: " + vault79Code)
    EndIf
EndEvent
