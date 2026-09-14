Function Fragment_Stage_0100_Item_00()
    If Scene_Intro && !Scene_Intro.IsPlaying()
        Scene_Intro.Start()
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    If Scene_Instructions && !Scene_Instructions.IsPlaying()
        Scene_Instructions.Start()
    EndIf
EndFunction

Function Fragment_Stage_0190_Item_00()
    If Scene_LastCall && !Scene_LastCall.IsPlaying()
        Scene_LastCall.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    If Scene_Takeover && !Scene_Takeover.IsPlaying()
        Scene_Takeover.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    If Scene_Wave1 && !Scene_Wave1.IsPlaying()
        Scene_Wave1.Start()
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    If Scene_Wave1Complete && !Scene_Wave1Complete.IsPlaying()
        Scene_Wave1Complete.Start()
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    If Scene_Wave2Complete && !Scene_Wave2Complete.IsPlaying()
        Scene_Wave2Complete.Start()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    If Scene_QuestComplete && !Scene_QuestComplete.IsPlaying()
        Scene_QuestComplete.Start()
    EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
    If Scene_QuestFail && !Scene_QuestFail.IsPlaying()
        Scene_QuestFail.Start()
    EndIf
EndFunction

Function Fragment_Stage_9991_Item_00()
    If Scene_QuestFail && !Scene_QuestFail.IsPlaying()
        Scene_QuestFail.Start()
    EndIf
EndFunction

Function Fragment_Stage_9992_Item_00()
    If Scene_QuestFail && !Scene_QuestFail.IsPlaying()
        Scene_QuestFail.Start()
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
    Stop()
EndFunction
