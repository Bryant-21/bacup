Function Fragment_Terminal_02()
    If pBoS02 != None && pBoS02.GetStage() >= 1400 && pBoS02.GetStage() < 1450
        pBoS02.SetStage(1450)
    EndIf
EndFunction
