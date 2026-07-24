Event OnRead()
    If pBoS01 != None && pBoS01.GetStage() < 300
        pBoS01.SetStage(300)
    EndIf
EndEvent
