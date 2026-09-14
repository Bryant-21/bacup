Actor Function ArenaPlayerReference()
    If PlayerRef != None
        Actor aliasedPlayer = PlayerRef.GetActorReference()
        If aliasedPlayer != None
            Return aliasedPlayer
        EndIf
    EndIf
    Return Game.GetPlayer()
EndFunction

Bool Function CheersAreActive()
    If StageToBeginCheers <= 0 || !IsStageDone(StageToBeginCheers)
        Return False
    EndIf
    Return StageToEndCheers <= 0 || !IsStageDone(StageToEndCheers)
EndFunction

Bool Function KillIsInTheArena(Actor akCrowd, Actor akVictim)
    If ArenaLocation != None
        Return akCrowd.IsInLocation(ArenaLocation)
    EndIf
    If CheerSFXRadius <= 0.0 || akVictim == None
        Return True
    EndIf
    Return akVictim.GetDistance(akCrowd) <= CheerSFXRadius
EndFunction

Function PlayRandomCheer(Actor akCrowd)
    If CheerSoundEffects == None || CheerSoundEffects.Length == 0
        Return
    EndIf
    Sound cheer = CheerSoundEffects[Utility.RandomInt(0, CheerSoundEffects.Length - 1)]
    If cheer != None
        cheer.Play(akCrowd)
    EndIf
EndFunction

Function WatchArenaKills()
    Actor player = ArenaPlayerReference()
    If player != None
        RegisterForRemoteEvent(player, "OnKill")
        RegisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
EndFunction

Event OnQuestInit()
    WatchArenaKills()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    WatchArenaKills()
EndEvent

Event Actor.OnKill(Actor akSender, Actor akVictim)
    If akSender == None || akSender != ArenaPlayerReference()
        Return
    EndIf
    If !CheersAreActive() || !KillIsInTheArena(akSender, akVictim)
        Return
    EndIf
    If Utility.RandomFloat(0.0, 1.0) > ChanceToCheerOnKill
        Return
    EndIf
    PlayRandomCheer(akSender)
EndEvent

Event OnQuestShutdown()
    Actor player = ArenaPlayerReference()
    If player != None
        UnregisterForRemoteEvent(player, "OnKill")
        UnregisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
EndEvent
