Function Fragment_Stage_0200_Item_00()
    If PA_TruckExploded && !PA_TruckExploded.IsPlaying()
        PA_TruckExploded.Start()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    If PA_StillDestroyed && !PA_StillDestroyed.IsPlaying()
        PA_StillDestroyed.Start()
    EndIf
EndFunction

Function Fragment_Stage_0810_Item_00()
    If PA_StillDestroyed && !PA_StillDestroyed.IsPlaying()
        PA_StillDestroyed.Start()
    EndIf
EndFunction

Function Fragment_Stage_0820_Item_00()
    If PA_StillDestroyed && !PA_StillDestroyed.IsPlaying()
        PA_StillDestroyed.Start()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    If PA_VenomDeposited && !PA_VenomDeposited.IsPlaying()
        PA_VenomDeposited.Start()
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    If PA_VenomHalfway && !PA_VenomHalfway.IsPlaying()
        PA_VenomHalfway.Start()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    If PA_RequiredVenomGoal && !PA_RequiredVenomGoal.IsPlaying()
        PA_RequiredVenomGoal.Start()
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    If PA_ExtraVenomGoal && !PA_ExtraVenomGoal.IsPlaying()
        PA_ExtraVenomGoal.Start()
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    If PA_EventSuccess && !PA_EventSuccess.IsPlaying()
        PA_EventSuccess.Start()
    EndIf
EndFunction

Function Fragment_Stage_3000_Item_00()
    If PA_EventFailure && !PA_EventFailure.IsPlaying()
        PA_EventFailure.Start()
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
    Stop()
EndFunction
