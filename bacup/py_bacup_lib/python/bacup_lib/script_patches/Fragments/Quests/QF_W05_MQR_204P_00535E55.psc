; TODO

Function Fragment_Stage_0002_Item_00()
    If Alias_InitEnableMarker != None
        ObjectReference initMarker = Alias_InitEnableMarker.GetReference()
        If initMarker != None
            initMarker.Enable()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    W05_MQR_204P_QuestScript questScript = Self as W05_MQR_204P_QuestScript
    If questScript == None
        Return
    EndIf

    If questScript.LevHideoutEnableMarker != None
        ObjectReference enableMarker = questScript.LevHideoutEnableMarker.GetReference()
        If enableMarker != None
            enableMarker.Enable()
        EndIf
    EndIf
    If questScript.LevHideoutEncounterEnableMarker != None
        ObjectReference encounterMarker = questScript.LevHideoutEncounterEnableMarker.GetReference()
        If encounterMarker != None
            encounterMarker.Enable()
        EndIf
    EndIf
    If questScript.LevHideoutLayoutEnableMarker != None
        ObjectReference layoutMarker = questScript.LevHideoutLayoutEnableMarker.GetReference()
        If layoutMarker != None
            layoutMarker.Enable()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
    If Alias_Rocco != None
        ObjectReference roccoRef = Alias_Rocco.GetReference()
        If roccoRef != None
            roccoRef.Enable()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
    If Alias_InitEnableMarker == None || Alias_Lou == None
        Return
    EndIf

    ObjectReference initMarker = Alias_InitEnableMarker.GetReference()
    Actor louRef = Alias_Lou.GetActorReference()
    If initMarker == None || louRef == None
        Return
    EndIf
    If !IsStageDone(2)
        SetStage(2)
    EndIf
    If IsStageDone(2) && !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_0151_Item_00()
    SetObjectiveCompleted(150)
    If Alias_Lou == None || Alias_Creature == None || !IsStageDone(2)
        Return
    EndIf

    Actor louRef = Alias_Lou.GetActorReference()
    Actor creatureRef = Alias_Creature.GetActorReference()
    If louRef != None && creatureRef != None && !IsStageDone(160)
        SetStage(160)
    EndIf
EndFunction

Function Fragment_Stage_0160_Item_00()
    SetObjectiveCompleted(150)
    SetObjectiveDisplayed(160)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(160)
    SetObjectiveDisplayed(200)
    If Alias_Lou == None || GetUpScene == None
        Return
    EndIf

    Actor louRef = Alias_Lou.GetActorReference()
    If louRef == None
        Return
    EndIf
    If TiedUpScene != None
        TiedUpScene.Stop()
    EndIf
    GetUpScene.Start()
    SetObjectiveCompleted(200)
    SetStage(300)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveCompleted(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(500)
    SetObjectiveDisplayed(510)
    SetObjectiveDisplayed(520)
    SetObjectiveDisplayed(530)
    SetObjectiveDisplayed(540)
EndFunction

Function Fragment_Stage_0510_Item_00()
    SetObjectiveCompleted(510)
    If IsStageDone(520) && IsStageDone(530) && !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
    SetObjectiveCompleted(520)
    If IsStageDone(510) && IsStageDone(530) && !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0530_Item_00()
    SetObjectiveCompleted(530)
    If IsStageDone(510) && IsStageDone(520) && !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0540_Item_00()
    SetObjectiveCompleted(540)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQR_204P_LevHideoutActiveValue, 1.0)
    EndIf

    W05_MQR_204P_QuestScript questScript = Self as W05_MQR_204P_QuestScript
    If questScript == None || Alias_Lev == None || Alias_Fisher == None || Alias_Surge == None
        Return
    EndIf
    If questScript.LevHideoutEnableMarker == None || questScript.LevHideoutEncounterEnableMarker == None
        Return
    EndIf
    If questScript.LevHideoutLayoutEnableMarker == None
        Return
    EndIf

    ObjectReference enableMarker = questScript.LevHideoutEnableMarker.GetReference()
    ObjectReference encounterMarker = questScript.LevHideoutEncounterEnableMarker.GetReference()
    ObjectReference layoutMarker = questScript.LevHideoutLayoutEnableMarker.GetReference()
    Actor levRef = Alias_Lev.GetActorReference()
    Actor fisherRef = Alias_Fisher.GetActorReference()
    Actor surgeRef = Alias_Surge.GetActorReference()
    If enableMarker == None || encounterMarker == None || layoutMarker == None
        Return
    EndIf
    If levRef == None || fisherRef == None || surgeRef == None
        Return
    EndIf
    If !IsStageDone(3)
        SetStage(3)
    EndIf
    If IsStageDone(3) && !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0810_Item_00()
    If IsStageDone(820) && !IsStageDone(830)
        SetStage(830)
    EndIf
EndFunction

Function Fragment_Stage_0820_Item_00()
    If IsStageDone(810) && !IsStageDone(830)
        SetStage(830)
    EndIf
EndFunction

Function Fragment_Stage_0830_Item_00()
    SetObjectiveCompleted(800)
    SetObjectiveDisplayed(840)
EndFunction

Function Fragment_Stage_0840_Item_00()
    SetObjectiveCompleted(840)
EndFunction

Function Fragment_Stage_0850_Item_00()
    If Alias_Lev == None
        Return
    EndIf

    Actor levRef = Alias_Lev.GetActorReference()
    If levRef == None
        Return
    EndIf
    levRef.StopCombat()
    levRef.ResetHealthAndLimbs()
    levRef.EvaluatePackage()
    SetStage(900)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
EndFunction

Function Fragment_Stage_0970_Item_00()
    If Alias_Lev == None
        Return
    EndIf

    Actor levRef = Alias_Lev.GetActorReference()
    Actor playerRef = Game.GetPlayer()
    If levRef != None && playerRef != None
        levRef.StartCombat(playerRef, True)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(900)
    SetObjectiveDisplayed(1000)
    If Alias_Detonator == None
        Return
    EndIf

    ObjectReference detonatorRef = Alias_Detonator.GetReference()
    Actor playerRef = Game.GetPlayer()
    If detonatorRef != None && playerRef != None
        playerRef.AddItem(detonatorRef, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(1000)
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_5000_Item_00()
    SetObjectiveDisplayed(5000)
EndFunction

Function Fragment_Stage_5100_Item_00()
    SetObjectiveDisplayed(5100)
EndFunction

Function Fragment_Stage_5200_Item_00()
    SetObjectiveDisplayed(5200)
EndFunction

Function Fragment_Stage_5300_Item_00()
    SetObjectiveDisplayed(5300)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(1100)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQ_204P_FactionChosen, 1.0)
    EndIf
    If W05_MQR_205P_QuestStart_Keyword != None
        W05_MQR_205P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
