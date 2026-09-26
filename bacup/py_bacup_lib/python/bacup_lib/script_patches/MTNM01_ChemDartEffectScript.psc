Event OnEffectStart(Actor akTarget, Actor akCaster)
    MTNM01QuestScript controller = MTNM01_Mayhem as MTNM01QuestScript
    If !controller || !controller.IsRunning() || controller.IsStageDone(controller.UseChemDartStage + 50) || !controller.IsStageDone(controller.UseChemDartStage)
        Return
    EndIf
    If !akTarget || akTarget.IsDead() || akCaster != Game.GetPlayer()
        Return
    EndIf

    If controller.KarmaCreature
        controller.KarmaCreature.ForceRefTo(akTarget)
    EndIf

    Actor playerRef = Game.GetPlayer()
    Race targetRace = akTarget.GetRace()
    If akTarget == playerRef || targetRace == HumanRace
        controller.SetStage(controller.KarmaPlayerStage)
    ElseIf akTarget.HasKeyword(ActorTypeRobot)
        controller.SetStage(controller.KarmaRobotStage)
    ElseIf targetRace == YaoGuaiRace || akTarget.GetLeveledActorBase() == EncYaoGuai00
        controller.SetStage(controller.KarmaYaoGuaiStage)
    ElseIf akTarget.GetLevel() < playerRef.GetLevel()
        controller.SetStage(controller.KarmaEasyStage)
    ElseIf akTarget.GetLevel() > playerRef.GetLevel()
        controller.SetStage(controller.KarmaDifficultStage)
    Else
        controller.SetStage(controller.KarmaOtherStage)
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    ; Stage 352 ("failed to kill target in time") had no setter, so a Karma target
    ; that survived the effect left the quest stuck at the stage-350 kill objective.
    MTNM01QuestScript controller = MTNM01_Mayhem as MTNM01QuestScript
    If !controller || !controller.IsRunning() || !controller.IsStageDone(350) || controller.IsStageDone(351) || controller.IsStageDone(352)
        Return
    EndIf
    If !akTarget || akTarget.IsDead() || !controller.KarmaCreature || controller.KarmaCreature.GetActorReference() != akTarget
        Return
    EndIf
    controller.SetStage(352)
EndEvent
