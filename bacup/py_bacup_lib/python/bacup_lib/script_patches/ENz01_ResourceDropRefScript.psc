; The orbital drop crate also carries Default1StateSyncActivator (JumpState02,
; DropSpeed, 16.6 seconds). The quest controller starts the drop and polls
; completion; this script owns the fly-by, the touchdown explosion at the
; retained progress fraction and the camera shake. bLockOnUnload is not
; emulated: the controller keeps the crate locked until stage 100.
Function ENz01_BeginDrop()
    B21TouchdownPlayed = False
    Default1StateSyncActivator sync = (Self as ObjectReference) as Default1StateSyncActivator
    If sync != None
        sync.ResetSyncAnimation()
        sync.BeginSyncAnimation()
    EndIf
    Actor playerRef = Game.GetPlayer()
    If FXOrbitalCacheFlyBy != None && playerRef != None && ENz01_WithinRange(playerRef, ENz01_DropCrateAudioDistance)
        FXOrbitalCacheFlyBy.Play(Self)
    EndIf
    StartTimer(0.5, iTouchdownProgressID)
EndFunction

Float Function ENz01_DropProgress()
    Default1StateSyncActivator sync = (Self as ObjectReference) as Default1StateSyncActivator
    If sync == None
        Return 1.0
    EndIf
    Return sync.GetSyncAnimationProgress()
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != iTouchdownProgressID || B21TouchdownPlayed || IsDisabled()
        Return
    EndIf
    If ENz01_DropProgress() >= fDropTouchdownProgress
        ENz01_PlayTouchdown()
    Else
        StartTimer(0.5, iTouchdownProgressID)
    EndIf
EndEvent

Function ENz01_PlayTouchdown()
    If B21TouchdownPlayed
        Return
    EndIf
    B21TouchdownPlayed = True
    If EN_ExplosionOrbitalDropTouchdown != None
        PlaceAtMe(EN_ExplosionOrbitalDropTouchdown, 1, False, False, False)
    EndIf
    Actor playerRef = Game.GetPlayer()
    If EN_CameraShakeSpell != None && playerRef != None && ENz01_WithinRange(playerRef, EN_CameraShakeRange)
        EN_CameraShakeSpell.Cast(Self, playerRef)
    EndIf
EndFunction

Function ENz01_EndDrop()
    CancelTimer(iTouchdownProgressID)
    B21TouchdownPlayed = False
    Default1StateSyncActivator sync = (Self as ObjectReference) as Default1StateSyncActivator
    If sync != None
        sync.ResetSyncAnimation()
    EndIf
EndFunction

Bool Function ENz01_WithinRange(Actor akPlayer, GlobalVariable akRange)
    If akRange == None || akRange.GetValue() <= 0.0
        Return True
    EndIf
    Return akPlayer.GetDistance(Self) <= akRange.GetValue()
EndFunction
