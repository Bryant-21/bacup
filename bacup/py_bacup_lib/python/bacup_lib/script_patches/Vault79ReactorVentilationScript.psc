Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || IsActivationBlocked()
        Return
    EndIf

    BlockActivation(True, True)
    myKlaxonDummy.Activate(akActionRef)
    myDoorDummy.Activate(akActionRef)

    Utility.Wait(4.0)

    myFanOffEnableTrigger.Disable()
    myFanOnEnableTrigger.Enable()
    myKillTrigger.Enable()
    myRadDisableTrigger.Disable()
    myVentilationSoundRef.Enable()
    myMachineHumRef.Enable()
    myFanOnSound.Play(myFanOnSoundRef)
    myFanOnSound2.Play(myFanOnSoundRef)
    mySecurityDummy.Activate(akActionRef)

    Quest secretsRevealed = Game.GetFormFromFile(0x0054EDB9, "SeventySix.esm") as Quest
    If secretsRevealed != None && secretsRevealed.IsStageDone(400) && !secretsRevealed.IsStageDone(450)
        secretsRevealed.SetStage(450)
    EndIf

    myDoorDummy.Activate(akActionRef)
    myKlaxonDummy.Activate(akActionRef)
EndEvent
