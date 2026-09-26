Function Fragment_Stage_0020_Item_00()
    If GHL00_Quest != None && !GHL00_Quest.GetStageDone(200)
        GHL00_Quest.SetStage(200)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction
