; SSE_BigBloom_QuestScript owns waves, timers and resets; these fragments own objectives and the finale staging.
; Stages 1/3/5 are FO76 debug skips and stay empty.
SSE_BigBloom_QuestScript Function Controller()
    Return (Self as Quest) as SSE_BigBloom_QuestScript
EndFunction

Function Fragment_Stage_0001_Item_00()
EndFunction

Function Fragment_Stage_0003_Item_00()
EndFunction

Function Fragment_Stage_0005_Item_00()
EndFunction

Function Fragment_Stage_0100_Item_00()
    SSE_BigBloom_QuestScript ctrl = Controller()
    If ctrl != None
        ctrl.StartEvent()
    EndIf
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0125_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(15, True)
EndFunction

Function Fragment_Stage_0127_Item_00()
    SetObjectiveCompleted(15, True)
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("Radlilies", 0.0)
        questVariables.SetVariable("Crystalcups", 0.0)
        questVariables.SetVariable("CarnalWeepers", 0.0)
        questVariables.SetVariable("Overgrown", 0.0)
    EndIf
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(30, True)
    SetObjectiveDisplayed(40, True)
    SetObjectiveDisplayed(50, True)
    SetStage(130)
EndFunction

Function Fragment_Stage_0130_Item_00()
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(30, True)
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveCompleted(20, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(40, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveCompleted(30, True)
    SetObjectiveCompleted(40, True)
    SetObjectiveCompleted(50, True)
    SetStage(710)
EndFunction

Function Fragment_Stage_0710_Item_00()
    If OvergrownBurnMessage != None
        OvergrownBurnMessage.Show()
    EndIf
    SetObjectiveDisplayed(60, True)
    SetObjectiveDisplayed(65, True)
EndFunction

Function Fragment_Stage_0720_Item_00()
    SetObjectiveCompleted(65, True)
EndFunction

Function Fragment_Stage_0750_Item_00()
    SetObjectiveCompleted(60, True)
    If !IsObjectiveCompleted(65)
        SetObjectiveDisplayed(65, False)
    EndIf
    SetObjectiveDisplayed(70, True)
EndFunction

Function Fragment_Stage_0770_Item_00()
    SetObjectiveCompleted(70, True)
    SetObjectiveDisplayed(80, True)
EndFunction

Function Fragment_Stage_0775_Item_00()
    Actor player = Game.GetPlayer()
    If ExplosionSource != None && ExplosionSource.GetReference() != None && BeezlebabyEntranceExplosion != None
        ExplosionSource.GetReference().PlaceAtMe(BeezlebabyEntranceExplosion)
    EndIf
    EnableCollection(FinaleBeezlebabies)
    EnableCollection(beeSwarms)
    If cameraShake != None
        cameraShake.Cast(player, player)
    EndIf
    If HoneybeastSFX02 != None
        HoneybeastSFX02.Play(player)
    EndIf
EndFunction

Function Fragment_Stage_0780_Item_00()
    Actor player = Game.GetPlayer()
    SetObjectiveCompleted(80, True)
    ObjectReference marker = None
    If Alias_BeezlebubMarker != None
        marker = Alias_BeezlebubMarker.GetReference()
    EndIf
    If marker != None && SSE_Beezlebub != None && Alias_Beezlebub != None
        If BeezlebubExplosion != None
            marker.PlaceAtMe(BeezlebubExplosion)
        EndIf
        Actor boss = marker.PlaceActorAtMe(SSE_Beezlebub)
        Alias_Beezlebub.ForceRefTo(boss)
    EndIf
    If cameraShakeIntense != None
        cameraShakeIntense.Cast(player, player)
    EndIf
    If HoneybeastSFX01 != None
        HoneybeastSFX01.Play(player)
    EndIf
    SetObjectiveDisplayed(90, True)
    SetStage(800)
EndFunction

Function Fragment_Stage_0800_Item_00()
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
    If Alias_Beezlebub != None
        ObjectReference boss = Alias_Beezlebub.GetReference()
        Alias_Beezlebub.Clear()
        If boss != None
            boss.Disable()
            boss.Delete()
        EndIf
    EndIf
EndFunction

Function EnableCollection(RefCollectionAlias akCollection)
    If akCollection == None
        Return
    EndIf
    Int index = 0
    While index < akCollection.GetCount()
        ObjectReference target = akCollection.GetAt(index)
        If target != None
            target.Enable()
        EndIf
        index += 1
    EndWhile
EndFunction
