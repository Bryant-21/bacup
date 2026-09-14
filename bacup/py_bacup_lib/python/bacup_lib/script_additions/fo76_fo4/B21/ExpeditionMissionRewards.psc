Scriptname B21:ExpeditionMissionRewards Extends Quest
{Grants the deterministic single-player completion tier for converted Expeditions.}

Int[] Property OptionalStages Auto Const
{The three optional objective success stages for this mission.}

GlobalVariable[] Property TierXP Auto
{XP amount globals for zero through three completed optional objectives.}

Int[] Property TierStampCounts Auto Const
{Stamp counts for zero through three completed optional objectives.}

MiscObject Property StampItem Auto
{The converted Stamp inventory item.}

Bool RewardGrantedThisRun

Event OnQuestInit()
    RewardGrantedThisRun = False
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 0
        RewardGrantedThisRun = False
    ElseIf auiStageID == 9000
        GrantCompletionReward()
    EndIf
EndEvent

Function GrantCompletionReward()
    If RewardGrantedThisRun || !RewardContractIsValid()
        Return
    EndIf

    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf

    Int tier = CompletedOptionalCount()
    GlobalVariable xpAmount = TierXP[tier]
    Int stampCount = TierStampCounts[tier]
    If xpAmount == None || xpAmount.GetValueInt() <= 0 || stampCount <= 0
        Return
    EndIf

    RewardGrantedThisRun = True
    Game.RewardPlayerXP(xpAmount.GetValueInt())
    playerRef.AddItem(StampItem, stampCount, True)
EndFunction

Int Function CompletedOptionalCount()
    Int completed = 0
    Int index = 0
    While index < OptionalStages.Length
        If IsStageDone(OptionalStages[index])
            completed += 1
        EndIf
        index += 1
    EndWhile
    Return completed
EndFunction

Bool Function RewardContractIsValid()
    Return OptionalStages != None && OptionalStages.Length == 3 && TierXP != None && TierXP.Length == 4 && TierStampCounts != None && TierStampCounts.Length == 4 && StampItem != None
EndFunction
