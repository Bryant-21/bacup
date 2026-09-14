Function Fragment_Stage_0001_Item_00()
    SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0005_Item_00()
    SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(15, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(15, True)
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(40, True)
    SetObjectiveDisplayed(50, True)
    SetObjectiveDisplayed(60, True)
    SetObjectiveDisplayed(70, True)
EndFunction

Function Fragment_Stage_0040_Item_00()
    (Alias_MTR02_MinerPlayer.GetReference() as Actor).SetValue(MTR02_Miner_LeftArmDone, 1.0)
EndFunction

Function Fragment_Stage_0050_Item_00()
    (Alias_MTR02_MinerPlayer.GetReference() as Actor).SetValue(MTR02_Miner_RightArmDone, 1.0)
EndFunction

Function Fragment_Stage_0060_Item_00()
    (Alias_MTR02_MinerPlayer.GetReference() as Actor).SetValue(MTR02_Miner_HeadDone, 1.0)
    SetObjectiveCompleted(60, True)
EndFunction

Function Fragment_Stage_0070_Item_00()
    (Alias_MTR02_MinerPlayer.GetReference() as Actor).SetValue(MTR02_Miner_TorsoDone, 1.0)
    SetObjectiveCompleted(70, True)
EndFunction

Function Fragment_Stage_0080_Item_00()
    (Alias_MTR02_MinerPlayer.GetReference() as Actor).SetValue(MTR02_Miner_LeftLegDone, 1.0)
EndFunction

Function Fragment_Stage_0090_Item_00()
    (Alias_MTR02_MinerPlayer.GetReference() as Actor).SetValue(MTR02_Miner_RightLegDone, 1.0)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(40, True)
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(50, True)
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveCompleted(40, True)
    SetObjectiveCompleted(50, True)
    SetObjectiveCompleted(60, True)
    SetObjectiveCompleted(70, True)
    SetObjectiveDisplayed(80, True)
EndFunction

Function Fragment_Stage_0255_Item_00()
    SetObjectiveCompleted(80, True)
    CompleteQuest()
    Stop()
EndFunction
