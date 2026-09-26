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
    Return 3
EndFunction

Int Function EffectiveKnifeTimerId()
    If KnifeDelayTimerId > 0
        Return KnifeDelayTimerId
    EndIf
    Return 2
EndFunction

Event OnLoad()
    ; Loading the cell must not advance an unactivated vent chain.
    Return
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If bCrawlPlaying
        Return
    EndIf
    bCrawlPlaying = True
    If RaRaDustFXKeyword != None && GetLinkedRef(RaRaDustFXKeyword) != None
        StartTimer(DustDelayTimerLength, EffectiveDustTimerId())
    EndIf
    If myBugKillSound != None
        StartTimer(BugKillDelayTimerLength, EffectiveBugKillTimerId())
    EndIf
    If myKnifeSound != None || myKnifeVSFleshSound != None
        StartTimer(KnifeDelayTimerLength, EffectiveKnifeTimerId())
    EndIf
    If mySound != None
        mySound.PlayAndWait(Self)
    EndIf
    ObjectReference nextMarker = GetLinkedRef()
    If nextMarker != None && nextMarker != Self
        nextMarker.Activate(akActionRef)
    EndIf
    bCrawlPlaying = False
EndEvent

Event OnUnload()
    CancelTimer(EffectiveDustTimerId())
    CancelTimer(EffectiveBugKillTimerId())
    CancelTimer(EffectiveKnifeTimerId())
EndEvent

Event OnTimer(int aiTimerID)
    If aiTimerID == EffectiveDustTimerId() && RaRaDustFXKeyword != None
        Vault79FallingDustScript dustEffects = GetLinkedRef(RaRaDustFXKeyword) as Vault79FallingDustScript
        If dustEffects != None
            dustEffects.PlayDustEffects()
        EndIf
    ElseIf aiTimerID == EffectiveBugKillTimerId() && myBugKillSound != None
        If myBugKillMarker != None
            myBugKillSound.Play(myBugKillMarker)
        Else
            myBugKillSound.Play(Self)
        EndIf
    ElseIf aiTimerID == EffectiveKnifeTimerId() && (myKnifeSound != None || myKnifeVSFleshSound != None)
        If myKnifeSound != None
            myKnifeSound.Play(Self)
        EndIf
        If myKnifeVSFleshSound != None
            myKnifeVSFleshSound.Play(Self)
        EndIf
    EndIf
EndEvent
