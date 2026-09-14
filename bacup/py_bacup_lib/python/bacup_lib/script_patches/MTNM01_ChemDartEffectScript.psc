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
