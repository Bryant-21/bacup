Function Fragment_Stage_0005_Item_00()
    SetObjectiveDisplayed(10)
    SetObjectiveDisplayed(20)
    SetObjectiveDisplayed(30)
    ENz01_AboveScript controller = (Self as Quest) as ENz01_AboveScript
    If controller != None
        controller.ENz01_BeginStartup()
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveCompleted(10)
    ENz01_DishOriented()
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(20)
    ENz01_DishOriented()
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(30)
    ENz01_DishOriented()
EndFunction

Function Fragment_Stage_0050_Item_00()
    ENz01_Say(ENz01_AllSitesOrientedTopic)
    ENz01_AboveScript controller = (Self as Quest) as ENz01_AboveScript
    If controller != None
        controller.ENz01_BeginOrbitalDrop()
    EndIf
EndFunction

Function Fragment_Stage_0055_Item_00()
    SetObjectiveDisplayed(55)
    ENz01_AboveScript controller = (Self as Quest) as ENz01_AboveScript
    If controller != None
        controller.ENz01_BeginDefense()
    EndIf
EndFunction

; The wave has finished spawning; the objective already tracks the collection.
Function Fragment_Stage_0090_Item_00()
    SetObjectiveDisplayed(55)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(55)
    SetObjectiveDisplayed(100)
    ENz01_Say(ENz01_UnlockResourceDrop)
    ENz01_AboveScript controller = (Self as Quest) as ENz01_AboveScript
    If controller != None
        Float lootSeconds = 0.0
        If ENz01_FinalQuestTimerLength != None
            lootSeconds = ENz01_FinalQuestTimerLength.GetValue()
        EndIf
        controller.ENz01_UnlockDrop(lootSeconds)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(100)
    EnclaveEventQuestScript eventQuest = (Self as Quest) as EnclaveEventQuestScript
    If eventQuest != None
        eventQuest.ENEvent_RecordCompletion()
    EndIf
    ENz01_AboveScript controller = (Self as Quest) as ENz01_AboveScript
    If controller != None
        controller.ENz01_BeginWrapUp()
    EndIf
EndFunction

Function Fragment_Stage_0160_Item_00()
    ENz01_Say(ENz01_FailureLine)
    ENz01_FailOpenObjectives()
    ENz01_AboveScript controller = (Self as Quest) as ENz01_AboveScript
    If controller != None
        controller.ENz01_BeginWrapUp()
    EndIf
EndFunction

; The loot window closed without the container being opened.
Function Fragment_Stage_0165_Item_00()
    ENz01_FailOpenObjectives()
    ENz01_AboveScript controller = (Self as Quest) as ENz01_AboveScript
    If controller != None
        controller.ENz01_CloseDrop()
        controller.ENz01_BeginWrapUp()
    EndIf
EndFunction

Function Fragment_Stage_0999_Item_00()
    ENz01_AboveScript controller = (Self as Quest) as ENz01_AboveScript
    If controller != None
        controller.ENz01_Shutdown()
    EndIf
EndFunction

Function ENz01_DishOriented()
    ENz01_AboveScript controller = (Self as Quest) as ENz01_AboveScript
    If controller != None && !controller.ENz01_RecordOrientation()
        ENz01_Say(ENz01_SiteOrientedTopic)
    EndIf
EndFunction

Function ENz01_Say(Topic akTopic)
    EnclaveEventQuestScript eventQuest = (Self as Quest) as EnclaveEventQuestScript
    If eventQuest != None
        eventQuest.ENEvent_SayToPlayer(akTopic)
    EndIf
EndFunction

Function ENz01_FailOpenObjectives()
    Int[] objectives = new Int[6]
    objectives[0] = 10
    objectives[1] = 20
    objectives[2] = 30
    objectives[3] = 55
    objectives[4] = 100
    objectives[5] = 100
    Int index = 0
    While index < objectives.Length
        If IsObjectiveDisplayed(objectives[index]) && !IsObjectiveCompleted(objectives[index])
            SetObjectiveFailed(objectives[index])
        EndIf
        index += 1
    EndWhile
EndFunction
