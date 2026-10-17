; Each of the three rounds (park, cottages, courtyard) runs: player lights the bonfire (x00), the eyebot reaches the next one (x70),
; the mischief meter fills (x80), the player is told to light the next pyre (x90).
Function EnableBonfire(ReferenceAlias akBonfire)
    ObjectReference bonfireRef = akBonfire.GetReference()
    If bonfireRef != None
        bonfireRef.Enable()
    EndIf
EndFunction

Function StartSceneIfStopped(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(5, True)
    EnableBonfire(Alias_Activator_Bonfire00)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(30, True)
    SetObjectiveDisplayed(50, True)
    SetObjectiveDisplayed(60, True)
    SetObjectiveDisplayed(70, True)
    StartSceneIfStopped(TravelToFirstBonfireScene)
EndFunction

Function Fragment_Stage_0270_Item_00()
    EnableBonfire(Alias_Activator_Bonfire01)
EndFunction

Function Fragment_Stage_0280_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveCompleted(50, True)
    SetObjectiveCompleted(60, True)
    SetObjectiveCompleted(70, True)
EndFunction

Function Fragment_Stage_0290_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(10, True)
    E03A_Mischief_ParkMessage.Show()
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(32, True)
    SetObjectiveDisplayed(50, True)
    SetObjectiveDisplayed(60, True)
    SetObjectiveDisplayed(80, True)
    SetObjectiveDisplayed(90, True)
    StartSceneIfStopped(TravelToSecondBonfireScene)
EndFunction

Function Fragment_Stage_0370_Item_00()
    EnableBonfire(Alias_Activator_Bonfire02)
EndFunction

Function Fragment_Stage_0380_Item_00()
    SetObjectiveCompleted(32, True)
    SetObjectiveCompleted(50, True)
    SetObjectiveCompleted(60, True)
    SetObjectiveCompleted(80, True)
    SetObjectiveCompleted(90, True)
EndFunction

Function Fragment_Stage_0390_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(10, True)
    E03A_Mischief_CottagesMessage.Show()
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(34, True)
    SetObjectiveDisplayed(50, True)
    SetObjectiveDisplayed(60, True)
    SetObjectiveDisplayed(80, True)
    SetObjectiveDisplayed(100, True)
    StartSceneIfStopped(TravelToFinalBonfireScene)
EndFunction

Function Fragment_Stage_0470_Item_00()
    EnableBonfire(Alias_Activator_Bonfire03)
EndFunction

Function Fragment_Stage_0480_Item_00()
    SetObjectiveCompleted(34, True)
    SetObjectiveCompleted(50, True)
    SetObjectiveCompleted(60, True)
    SetObjectiveCompleted(80, True)
    SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_0490_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(40, True)
    E03A_Mischief_CourtyardMessage.Show()
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(40, True)
    StartSceneIfStopped(FinalWaveScene)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9991_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9992_Item_00()
    FailAllObjectives()
    Stop()
EndFunction
