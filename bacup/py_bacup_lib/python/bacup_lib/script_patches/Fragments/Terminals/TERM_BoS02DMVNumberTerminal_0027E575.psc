Function Fragment_Terminal_01()
    If pBoS02 == None || pBoS02.GetStage() < 600 || pBoS02.GetStage() >= 800
        Return
    EndIf
    If pBoS02.GetStage() < 650
        pBoS02.SetStage(650)
    EndIf
    If pBoS02_DMV_Support != None && !pBoS02_DMV_Support.IsRunning()
        pBoS02_DMV_Support.Start()
    EndIf
EndFunction

Function Fragment_Terminal_02()
    If pBoS02 == None || pBoS02.GetStage() < 1200 || pBoS02.GetStage() >= 1400
        Return
    EndIf
    If pBoS02.GetStage() < 1300
        pBoS02.SetStage(1300)
    EndIf
    If pBoS02_DMV_Support != None && !pBoS02_DMV_Support.IsRunning()
        pBoS02_DMV_Support.Start()
    EndIf
EndFunction
