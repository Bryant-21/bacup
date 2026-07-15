Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    Int index = 0
    While index < EWSBossSolomonsPond.GetCount()
        Actor bossRef = EWSBossSolomonsPond.GetActorAt(index)
        If bossRef != None && bossRef != akSenderRef && !bossRef.IsDead()
            Return
        EndIf
        index += 1
    EndWhile

    Actor playerRef = MQ101APlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && W05_MQ_101P_A_RepairTerminalKey != None && playerRef.GetItemCount(W05_MQ_101P_A_RepairTerminalKey) < 1
        playerRef.AddItem(W05_MQ_101P_A_RepairTerminalKey, 1, True)
    EndIf
EndEvent
