Function ShowSignalStrength()
    BoS03Script controller = GetOwningQuest() as BoS03Script
    If controller != None
        controller.ShowTransponderSignalStrength()
    EndIf
EndFunction

Function Fragment_Phase_02_Begin()
    ShowSignalStrength()
EndFunction

Function Fragment_Phase_03_Begin()
    ShowSignalStrength()
EndFunction

Function Fragment_Phase_04_Begin()
    ShowSignalStrength()
EndFunction

Function Fragment_Phase_05_Begin()
    ShowSignalStrength()
EndFunction
