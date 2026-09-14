Event OnAliasInit()
    OwningQuest = GetOwningQuest()
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    If OwningQuest == None || !OwningQuest.IsStageDone(Stage_PieThrowingStarted) || OwningQuest.IsStageDone(Stage_PieThrowingCompleted)
        Return
    EndIf
    If akAggressor != Game.GetPlayer() || akSource != Weap_ThrowingPie
        Return
    EndIf
    Actor clown = akTarget as Actor
    Scene reaction = Scene_HitByPie_Male
    If clown != None && clown.GetActorBase().GetSex() == 1
        reaction = Scene_HitByPie_Female
    EndIf
    If reaction != None && !reaction.IsPlaying()
        reaction.Start()
    EndIf
    If AC_MQ02_Stage_AudienceReaction_PieThrowing != None && !AC_MQ02_Stage_AudienceReaction_PieThrowing.IsPlaying()
        AC_MQ02_Stage_AudienceReaction_PieThrowing.Start()
    EndIf
    OwningQuest.SetStage(Stage_PieThrowingCompleted)
EndEvent
