State dead
    Event OnBeginState(String asOldState)
        If victim == None || CriticalEffectType < 1 || CriticalEffectType > 3
            Return
        EndIf

        Int criticalStartStage = CriticalEffectType * 2 - 1
        Int criticalEndStage = criticalStartStage + 1
        Float totalDuration = EffectTimeLength
        If totalDuration < 0.0
            totalDuration = 0.0
        EndIf
        Float containerDelay = totalDuration - 0.5
        If containerDelay < 0.0
            containerDelay = 0.0
        EndIf

        victim.SetCriticalStage(criticalStartStage)
        If TargetEffectShader != None
            TargetEffectShader.Play(victim, totalDuration)
        EndIf

        If containerDelay > 0.0
            Utility.Wait(containerDelay)
        EndIf
        If ContainerToDrop != None
            victim.PlaceAtMe(ContainerToDrop as Form, 1, False, False, True)
        EndIf

        Float remainingDuration = totalDuration - containerDelay
        If remainingDuration > 0.0
            Utility.Wait(remainingDuration)
        EndIf
        victim.SetCriticalStage(criticalEndStage)
        If TargetEffectShader != None
            TargetEffectShader.Stop(victim)
        EndIf
        victim = None
    EndEvent
EndState
