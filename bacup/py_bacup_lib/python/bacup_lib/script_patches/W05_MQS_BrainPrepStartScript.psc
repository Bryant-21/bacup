Event OnActivate(ObjectReference akActionRef)
    Actor player = CurrentPlayer.GetActorRef()
    if player == None || akActionRef != player
        return
    endif

    Quest owningQuest = GetOwningQuest()
    if owningQuest != None && StageToSet > 0 && owningQuest.IsStageDone(StageToSet)
        return
    endif

    bool settingsCorrect = player.GetValue(W05_MQS_203P_PrepTempCorrect) >= 1.0
    settingsCorrect = settingsCorrect && player.GetValue(W05_MQS_203P_PrepPHCorrect) >= 1.0
    settingsCorrect = settingsCorrect && player.GetValue(W05_MQS_203P_PrepTimerCorrect) >= 1.0
    int brainPlacement = player.GetValue(W05_MQS_203P_PrepPlacement) as int
    MiscObject brainReward = None
    ActorValue preparedBrainFlag = None

    if settingsCorrect && brainPlacement == 1
        brainReward = W05_MQS_203P_BrainJarPrepped_Dias
        preparedBrainFlag = W05_MQS_203P_HasPreppedDiasBrain
    elseif settingsCorrect && brainPlacement == 2
        brainReward = W05_MQS_203P_BrainJarPrepped_Greg
        preparedBrainFlag = W05_MQS_203P_HasPreppedGregBrain
    elseif settingsCorrect && brainPlacement == 3
        brainReward = W05_MQS_203P_BrainJarPrepped_Gina
        preparedBrainFlag = W05_MQS_203P_HasPreppedGinaBrain
    else
        QST203PStoveCatchBuzzer.Play(BrainPrepStove.GetRef())
        W05_MQS_203P_011_BrainPrepFail.Start()
        return
    endif

    if player.GetValue(preparedBrainFlag) < 1.0
        if player.GetItemCount(brainReward) < 1
            player.AddItem(brainReward, 1)
        endif
        player.SetValue(preparedBrainFlag, 1.0)
    endif

    QST203PStoveCatchBuzzer.Play(BrainPrepStove.GetRef())
    W05_MQS_203P_012_BrainPrepPass.Start()
    if owningQuest != None && StageToSet > 0 && !owningQuest.IsStageDone(StageToSet)
        owningQuest.SetStage(StageToSet)
    endif
EndEvent
