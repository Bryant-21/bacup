Function InitializeLocalEquipmentTracking()
    If !IsRunning()
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    RegisterForRemoteEvent(playerRef, "OnItemEquipped")
    RegisterForRemoteEvent(playerRef, "OnItemUnequipped")
    RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    Int index = 0
    While MoMQuestList != None && index < MoMQuestList.Length
        If MoMQuestList[index].MoMQuest != None
            RegisterForRemoteEvent(MoMQuestList[index].MoMQuest, "OnStageSet")
        EndIf
        index += 1
    EndWhile
    ReconcileLocalProgress()
    ReconcileLocalEquipment()
EndFunction

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    ReconcileLocalProgress()
EndEvent

Function ReconcileLocalProgress()
    B21ProgressDirty = True
    If B21ProgressBusy || !IsRunning() || MoMQuestList == None
        Return
    EndIf
    B21ProgressBusy = True
    While B21ProgressDirty
        B21ProgressDirty = False
        Actor playerRef = Game.GetPlayer()
        Int index = 0
        While index < MoMQuestList.Length
            Quest trackedQuest = MoMQuestList[index].MoMQuest
            ActorValue checkpoint = MoMQuestList[index].MoMQuestCheckpointValue
            If trackedQuest != None && checkpoint != None && playerRef != None
                Int progress = trackedQuest.GetStage()
                If trackedQuest.IsStageDone(100)
                    progress = 100
                EndIf
                If progress >= 10 && progress <= 100 && playerRef.GetValue(checkpoint) < progress
                    playerRef.SetValue(checkpoint, progress)
                EndIf
            EndIf
            If index == 2
                MoM01QuestScript initiate = trackedQuest as MoM01QuestScript
                If initiate != None
                    initiate.ReconcileTerminalState()
                EndIf
            ElseIf index == 4
                MoM02AQuestScript phantom = trackedQuest as MoM02AQuestScript
                If phantom != None
                    phantom.ReconcileTerminalState()
                EndIf
            ElseIf index == 5
                MoM02BQuestScript blade = trackedQuest as MoM02BQuestScript
                If blade != None
                    blade.ReconcileTerminalState()
                EndIf
            ElseIf index == 6
                MoM02CQuestScript voice = trackedQuest as MoM02CQuestScript
                If voice != None
                    voice.ReconcileTerminalState()
                EndIf
            ElseIf index == 7
                MoM03QuestScript seeker = trackedQuest as MoM03QuestScript
                If seeker != None
                    seeker.ReconcileTerminalState()
                EndIf
            EndIf
            index += 1
        EndWhile
        ReconcileAuxiliaryQuests()
    EndWhile
    B21ProgressBusy = False
EndFunction

Function ReconcileAuxiliaryQuests()
    If MoMQuestList.Length <= 10
        Return
    EndIf
    Quest initiateQuest = MoMQuestList[2].MoMQuest
    MoMItemManagerQuestScript manager = MoMItemManager as MoMItemManagerQuestScript
    Actor playerRef = Game.GetPlayer()
    If initiateQuest != None && manager != None && playerRef != None
        If initiateQuest.IsStageDone(30) && playerRef.GetItemCount(manager.MoM_ClothesMistressOfMysteryWornVeil) > 0
            StartAuxiliaryQuest(9, 20)
        EndIf
        If initiateQuest.IsStageDone(100)
            StartAuxiliaryQuest(10, 10)
        EndIf
    EndIf
EndFunction

Function StartAuxiliaryQuest(Int aiIndex, Int aiStage)
    Quest auxiliaryQuest = MoMQuestList[aiIndex].MoMQuest
    If auxiliaryQuest == None || auxiliaryQuest.IsCompleted() || auxiliaryQuest.IsStageDone(100)
        Return
    EndIf
    If !auxiliaryQuest.IsRunning() && MoMQuestList[aiIndex].MoMQuestKeyword != None
        MoMQuestList[aiIndex].MoMQuestKeyword.SendStoryEventAndWait(None, Game.GetPlayer(), Game.GetPlayer())
    EndIf
    If auxiliaryQuest.IsRunning() && !auxiliaryQuest.IsStageDone(aiStage)
        auxiliaryQuest.SetStage(aiStage)
    EndIf
EndFunction

Function ReconcileLocalEquipment()
    MoMItemManagerQuestScript manager = MoMItemManager as MoMItemManagerQuestScript
    If IsRunning() && manager != None
        manager.ReconcileLocalEquipment()
    EndIf
EndFunction

Event OnQuestInit()
    InitializeLocalEquipmentTracking()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    InitializeLocalEquipmentTracking()
EndEvent

Event Actor.OnItemEquipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    If akSender == Game.GetPlayer()
        MoMItemManagerQuestScript manager = MoMItemManager as MoMItemManagerQuestScript
        If manager != None
            manager.NoteLocalEquipEvent(akBaseObject)
        EndIf
        ReconcileLocalEquipment()
    EndIf
EndEvent

Event Actor.OnItemUnequipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    If akSender == Game.GetPlayer()
        ReconcileLocalEquipment()
    EndIf
EndEvent

Event OnQuestShutdown()
    UnregisterForAllRemoteEvents()
EndEvent
