Function Fragment_Stage_0100_Item_00()
    If FSS02_Vigilant_RoverFixScene && !FSS02_Vigilant_RoverFixScene.IsPlaying()
        FSS02_Vigilant_RoverFixScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    If FSS02_Vigilant_RoverToPodScene && !FSS02_Vigilant_RoverToPodScene.IsPlaying()
        FSS02_Vigilant_RoverToPodScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
    Stop()
EndFunction
