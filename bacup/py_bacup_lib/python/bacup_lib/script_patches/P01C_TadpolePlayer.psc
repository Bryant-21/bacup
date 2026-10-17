; Tadpole "Kindness: Revive an Ally": once the player has joined the Scouts (ReadyStage), healing a downed teammate
; (FO4's revive of an ally/companion) sets RevivedPlayerStage.
Event OnPlayerHealTeammate(Actor akTeammate)
    Quest owner = GetOwningQuest()
    If akTeammate != None && owner.IsStageDone(ReadyStage) && !owner.IsStageDone(RevivedPlayerStage)
        owner.SetStage(RevivedPlayerStage)
    EndIf
EndEvent
