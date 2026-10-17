; Item 1 volunteers for plant-life testing, item 2 for wildlife; the Cooked Samples science
; terminal shows the plant or meat analysis from plantORanimal 0 or 1.
Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Game.GetPlayer().SetValue(plantORanimal, 0.0)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    Game.GetPlayer().SetValue(plantORanimal, 1.0)
EndFunction
