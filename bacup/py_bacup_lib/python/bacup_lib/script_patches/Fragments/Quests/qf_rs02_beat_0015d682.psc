Function Fragment_Stage_0200_Item_00()
    If RS02_Beat_StartPatrol && !RS02_Beat_StartPatrol.IsPlaying()
        RS02_Beat_StartPatrol.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If RS02_Beat_Loc1AlarmScene && !RS02_Beat_Loc1AlarmScene.IsPlaying()
        RS02_Beat_Loc1AlarmScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    If RS02_Beat_Loc2TravelScene && !RS02_Beat_Loc2TravelScene.IsPlaying()
        RS02_Beat_Loc2TravelScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    If RS02_Beat_Loc2AlarmScene && !RS02_Beat_Loc2AlarmScene.IsPlaying()
        RS02_Beat_Loc2AlarmScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    If RS02_Beat_Loc3TravelScene && !RS02_Beat_Loc3TravelScene.IsPlaying()
        RS02_Beat_Loc3TravelScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    If RS02_Beat_Loc3AlarmScene && !RS02_Beat_Loc3AlarmScene.IsPlaying()
        RS02_Beat_Loc3AlarmScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1401_Item_00()
    If RS02_Beat_LocFinalTravelScene && !RS02_Beat_LocFinalTravelScene.IsPlaying()
        RS02_Beat_LocFinalTravelScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_5500_Item_00()
    If Scene_QuestFail && !Scene_QuestFail.IsPlaying()
        Scene_QuestFail.Start()
    EndIf
EndFunction

Function Fragment_Stage_6000_Item_00()
    Stop()
EndFunction
