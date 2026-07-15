Event OnAliasInit()
    RegisterForAnimationEvent(Game.GetPlayer(), "weaponDraw")
EndEvent

Event OnAliasShutdown()
    UnregisterForAnimationEvent(Game.GetPlayer(), "weaponDraw")
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
    Actor playerRef = Game.GetPlayer()
    If akSource != playerRef || asEventName != "weaponDraw"
        Return
    EndIf
    If playerRef.GetEquippedWeapon() != W05_MQ_003P_Muscle_PollyAssaultronHead
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && PrereqStage > 0 && owningQuest.IsStageDone(PrereqStage) && StageToSet > 0
        If !owningQuest.IsStageDone(StageToSet)
            owningQuest.SetStage(StageToSet)
        EndIf
    EndIf
EndEvent
