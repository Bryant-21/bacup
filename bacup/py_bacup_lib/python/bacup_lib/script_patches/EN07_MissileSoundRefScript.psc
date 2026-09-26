Function BeginLaunchEffects()
    ObjectReference missileRef = Self
    Default1StateSyncActivator animation = missileRef as Default1StateSyncActivator
    If B21LaunchEffectsActive || animation == None || animation.GetSyncAnimationProgress() >= fLaunchSoundProgress
        Return
    EndIf
    B21LaunchEffectsActive = True
    TriggerLaunchSound()
    StartTimer(0.1, iDoorAudioProgressID)
    StartTimer(0.1, iSpawnExplosionProgressID)
    StartTimer(0.1, iLaunchAudioProgressID)
EndFunction

Function CancelLaunchEffects()
    B21LaunchEffectsActive = False
    CancelTimer(iDoorAudioProgressID)
    CancelTimer(iSpawnExplosionProgressID)
    CancelTimer(iLaunchAudioProgressID)
    TriggerStopSound()
EndFunction

Event OnTimer(Int aiTimerID)
    ObjectReference missileRef = Self
    Default1StateSyncActivator animation = missileRef as Default1StateSyncActivator
    If !B21LaunchEffectsActive || animation == None
        Return
    EndIf
    Float progress = animation.GetSyncAnimationProgress()
    If aiTimerID == iDoorAudioProgressID
        If progress >= fDoorOpenSoundProgress
            TriggerDoorSound()
        Else
            StartTimer(0.1, aiTimerID)
        EndIf
    ElseIf aiTimerID == iSpawnExplosionProgressID
        If progress >= fSpawnExplosionProgress
            If Is3DLoaded() && ExplosionSpawn != None && EN07_MissileSiloExitExplosion != None
                ExplosionSpawn.PlaceAtMe(EN07_MissileSiloExitExplosion)
            EndIf
        Else
            StartTimer(0.1, aiTimerID)
        EndIf
    ElseIf aiTimerID == iLaunchAudioProgressID
        If progress >= fLaunchSoundProgress
            CancelLaunchEffects()
        Else
            StartTimer(0.1, aiTimerID)
        EndIf
    EndIf
EndEvent

Event OnLoad()
    ObjectReference missileRef = Self
    Default1StateSyncActivator animation = missileRef as Default1StateSyncActivator
    If B21LaunchEffectsActive && animation != None && animation.GetSyncAnimationProgress() < fLaunchSoundProgress
        TriggerLaunchSound()
    EndIf
EndEvent

Event OnUnload()
    TriggerStopSound()
EndEvent

Event OnReset()
    CancelLaunchEffects()
EndEvent

Function TriggerDoorSound()
    If DoorAudioToPlay != None && Is3DLoaded()
        DoorAudioToPlay.Play(Self)
    EndIf
EndFunction

Function TriggerStopSound()
    If B21HasSoundInstance
        Sound.StopInstance(iInstanceID)
        B21HasSoundInstance = False
        iInstanceID = -1
    EndIf
EndFunction

Function TriggerLaunchSound()
    TriggerStopSound()
    If SoundToPlay != None && Is3DLoaded()
        iInstanceID = SoundToPlay.Play(Self)
        B21HasSoundInstance = iInstanceID >= 0
    EndIf
EndFunction
