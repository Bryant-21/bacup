Function Fragment_Terminal_01()
    If pBoS02 != None && pBoS02.GetStage() >= 200 && pBoS02.GetStage() < 250
        pBoS02.SetStage(250)
    EndIf
EndFunction
