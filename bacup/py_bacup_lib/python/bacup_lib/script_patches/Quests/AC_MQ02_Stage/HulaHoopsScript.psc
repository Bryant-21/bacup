Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    PlayerRef = Alias_Player.GetActorReference()
EndEvent

Event OnTriggerEnter(ObjectReference akActionRef)
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    If OwningQuest == None || akActionRef == None || !OwningQuest.IsStageDone(Stage_GrenadeJugglingStarted) || OwningQuest.IsStageDone(Stage_GrenadeJugglingCompleted)
        Return
    EndIf
    If akActionRef.GetBaseObject() != AC_MQ02_Stage_JugglingGrenade_Projectile
        Return
    EndIf
    If AC_MQ02_Stage_AudienceReaction_GrenadeJuggling != None && !AC_MQ02_Stage_AudienceReaction_GrenadeJuggling.IsPlaying()
        AC_MQ02_Stage_AudienceReaction_GrenadeJuggling.Start()
    EndIf
    OwningQuest.SetStage(Stage_GrenadeJugglingCompleted)
EndEvent
