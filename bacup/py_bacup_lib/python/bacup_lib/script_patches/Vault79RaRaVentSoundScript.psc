Int Function EffectiveDustTimerId()
    If DustDelayTimerId > 0
        Return DustDelayTimerId
    EndIf
    Return 1
EndFunction

Int Function EffectiveBugKillTimerId()
    If BugKillDelayTimerId > 0
        Return BugKillDelayTimerId
    EndIf
    Return 2
EndFunction

Int Function EffectiveKnifeTimerId()
    If KnifeDelayTimerId > 0
        Return KnifeDelayTimerId
    EndIf
    Return 3
EndFunction

Event OnLoad()
    If mySound != None
        StartTimer(DustDelayTimerLength, EffectiveDustTimerId())
    EndIf
    If myBugKillSound != None
        StartTimer(BugKillDelayTimerLength, EffectiveBugKillTimerId())
    EndIf
    If myKnifeSound != None || myKnifeVSFleshSound != None
        StartTimer(KnifeDelayTimerLength, EffectiveKnifeTimerId())
    EndIf
EndEvent

Event OnUnload()
    If mySound != None
        CancelTimer(EffectiveDustTimerId())
    EndIf
    If myBugKillSound != None
        CancelTimer(EffectiveBugKillTimerId())
    EndIf
    If myKnifeSound != None || myKnifeVSFleshSound != None
        CancelTimer(EffectiveKnifeTimerId())
    EndIf
EndEvent

Event OnTimer(int aiTimerID)
    If aiTimerID == EffectiveDustTimerId() && mySound != None
        mySound.Play(Self)
        StartTimer(DustDelayTimerLength, EffectiveDustTimerId())
    ElseIf aiTimerID == EffectiveBugKillTimerId() && myBugKillSound != None
        If myBugKillMarker != None
            myBugKillSound.Play(myBugKillMarker)
        Else
            myBugKillSound.Play(Self)
        EndIf
        StartTimer(BugKillDelayTimerLength, EffectiveBugKillTimerId())
    ElseIf aiTimerID == EffectiveKnifeTimerId() && (myKnifeSound != None || myKnifeVSFleshSound != None)
        If myKnifeSound != None
            myKnifeSound.Play(Self)
        EndIf
        If myKnifeVSFleshSound != None
            myKnifeVSFleshSound.Play(Self)
        EndIf
        StartTimer(KnifeDelayTimerLength, EffectiveKnifeTimerId())
    EndIf
EndEvent
