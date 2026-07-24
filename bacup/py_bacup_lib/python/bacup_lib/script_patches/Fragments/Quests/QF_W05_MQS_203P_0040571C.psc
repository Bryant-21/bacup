; TODO

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
    If W05_MQS_203P_001_FlagScene != None
        W05_MQS_203P_001_FlagScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    If W05_MQS_203P_005_RobcoEntrance != None
        W05_MQS_203P_005_RobcoEntrance.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If W05_MQS_203P_006_RobCoFacilityScene != None
        W05_MQS_203P_006_RobCoFacilityScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    If W05_MQS_203P_007_DiscoverRobobrainScene != None
        W05_MQS_203P_007_DiscoverRobobrainScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    If W05_MQS_203P_009_EnterBrainRoom != None
        W05_MQS_203P_009_EnterBrainRoom.Start()
    EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_HasDiasBrain, 1.0)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJar_Dias) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJar_Dias, 1, False)
        EndIf
    EndIf
    If !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0720_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_HasGregBrain, 1.0)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJar_Greg) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJar_Greg, 1, False)
        EndIf
    EndIf
    If !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0730_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_HasGinaBrain, 1.0)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJar_Gina) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJar_Gina, 1, False)
        EndIf
    EndIf
    If !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    If W05_MQS_203P_010_EnterPrepScene != None
        W05_MQS_203P_010_EnterPrepScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_CanInteractBrainPrep, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1002_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_203P_BrainJar_Dias, 1, True)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJarPrepped_Dias) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJarPrepped_Dias, 1, False)
        EndIf
        playerRef.SetValue(W05_MQS_203P_HasPreppedDiasBrain, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1003_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_203P_BrainJar_Greg, 1, True)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJarPrepped_Greg) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJarPrepped_Greg, 1, False)
        EndIf
        playerRef.SetValue(W05_MQS_203P_HasPreppedGregBrain, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1004_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_203P_BrainJar_Gina, 1, True)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJarPrepped_Gina) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJarPrepped_Gina, 1, False)
        EndIf
        playerRef.SetValue(W05_MQS_203P_HasPreppedGinaBrain, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetItemCount(W05_MQS_203P_RobobrainDome) < 1
        playerRef.AddItem(W05_MQS_203P_RobobrainDome, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    Actor playerRef = Game.GetPlayer()
    Bool assembledChoice = False
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_203P_RobobrainDome, 1, True)
        If playerRef.GetValue(W05_MQS_203P_ChoseDias) > 0.0
            playerRef.RemoveItem(W05_MQS_203P_BrainJarPrepped_Dias, 1, True)
            assembledChoice = True
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGreg) > 0.0
            playerRef.RemoveItem(W05_MQS_203P_BrainJarPrepped_Greg, 1, True)
            assembledChoice = True
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGina) > 0.0
            playerRef.RemoveItem(W05_MQS_203P_BrainJarPrepped_Gina, 1, True)
            assembledChoice = True
        EndIf
    EndIf
    If assembledChoice && GetStage() < 1400
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If playerRef.GetValue(W05_MQS_203P_ChoseDias) > 0.0
            W05_MQS_203P_015A_DoctorDiasMakesTools.Start()
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGreg) > 0.0
            W05_MQS_203P_015C_GregMakesTools.Start()
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGina) > 0.0
            W05_MQS_203P_015B_GinaMakesTools.Start()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1510_Item_00()
    ObjectReference markerRef = Alias_EnableMarker_Tools_DiasVolatile.GetReference()
    If markerRef != None
        markerRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1520_Item_00()
    ObjectReference markerRef = Alias_EnableMarker_Tools_GregStandard.GetReference()
    If markerRef != None
        markerRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1530_Item_00()
    ObjectReference markerRef = Alias_EnableMarker_Tools_GinaClever.GetReference()
    If markerRef != None
        markerRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1800_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_QuestComplete, 1.0)
        playerRef.SetValue(W05_RadcliffIsInFoundation, 1.0)
    EndIf
    If W05_MQS_Choice_QuestStartKeyword != None
        W05_MQS_Choice_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
