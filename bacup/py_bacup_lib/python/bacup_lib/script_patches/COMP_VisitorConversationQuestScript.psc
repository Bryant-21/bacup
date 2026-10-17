Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If AddPlayerToQuest && Alias_Player != None && Alias_Player.GetReference() != playerRef
        Alias_Player.ForceRefTo(playerRef)
    EndIf
    If ObjectiveToDisplay_OnStart >= 0
        SetObjectiveDisplayed(ObjectiveToDisplay_OnStart, True)
    EndIf
    If PlayerAVToSet_OnStart != None
        playerRef.SetValue(PlayerAVToSet_OnStart, PlayerAVToSet_OnStart_Value)
    EndIf
EndEvent

Event OnQuestShutdown()
    If ObjectiveToComplete_OnShutdown >= 0
        SetObjectiveCompleted(ObjectiveToComplete_OnShutdown, True)
    EndIf
    If PlayerAVToSet_OnShutdown != None
        Game.GetPlayer().SetValue(PlayerAVToSet_OnShutdown, PlayerAVToSet_OnShutDown_Value)
    EndIf
    If SetDoneWithSceneAV && COMP_Visitor_DoneWithScene != None && Alias_Visitor != None
        Actor visitor = Alias_Visitor.GetActorReference()
        If visitor != None
            visitor.SetValue(COMP_Visitor_DoneWithScene, DoneWithSceneAV_Value)
        EndIf
    EndIf
EndEvent
