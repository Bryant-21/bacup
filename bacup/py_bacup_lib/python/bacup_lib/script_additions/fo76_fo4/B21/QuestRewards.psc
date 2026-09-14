Scriptname B21:QuestRewards Extends Quest
{Hands out the deterministic local reward rows this quest carried in FO76.

FO76 stores stage rewards in GMRW records hung off QUST.QRWD. The conversion
authors non-completion XP, caps, and explicit item rows onto this script.
Completion XP and notifications remain on FO4's native QUST.XNAM path. Reputation, bullion,
account currencies, entitlements, SCORE, and random legendary services are not
synthesized in single player.}

Int[] Property XPStages Auto Const
{Non-completion stages parallel to RewardXP.}

GlobalVariable[] Property RewardXP Auto
{XP amount globals parallel to XPStages.}

Int[] Property CapsStages Auto Const
{Stages carrying a locally resolvable caps row.}

GlobalVariable[] Property RewardCaps Auto
{Caps amount globals parallel to CapsStages.}

MiscObject Property CapsItem Auto
{FO4's bottlecap MISC.}

Int[] Property ItemStages Auto Const
{Stages parallel to RewardItems and RewardCounts.}

Form[] Property RewardItems Auto
{Explicit local reward payloads such as LVLI, MISC, ALCH, WEAP, or ARMO.}

Int[] Property RewardCounts Auto
{Parallel to RewardItems.}

Int[] GrantedStages

Event OnQuestInit()
    GrantedStages = None
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    GrantRewardsForStage(auiStageID)
EndEvent

Function GrantRewardsForStage(Int auiStageID)
    If !HasRewardForStage(auiStageID) || HasGrantedStage(auiStageID)
        Return
    EndIf

    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    RecordGrantedStage(auiStageID)

    Int index = 0
    If XPStages != None && RewardXP != None
        While index < XPStages.Length && index < RewardXP.Length
            GlobalVariable xpAmount = RewardXP[index]
            If XPStages[index] == auiStageID && xpAmount != None && xpAmount.GetValueInt() > 0
                Game.RewardPlayerXP(xpAmount.GetValueInt())
            EndIf
            index += 1
        EndWhile
    EndIf

    index = 0
    If CapsStages != None && RewardCaps != None && CapsItem != None
        While index < CapsStages.Length && index < RewardCaps.Length
            GlobalVariable capsAmount = RewardCaps[index]
            If CapsStages[index] == auiStageID && capsAmount != None && capsAmount.GetValueInt() > 0
                playerRef.AddItem(CapsItem, capsAmount.GetValueInt(), True)
            EndIf
            index += 1
        EndWhile
    EndIf

    index = 0
    If ItemStages != None && RewardItems != None && RewardCounts != None
        While index < ItemStages.Length && index < RewardItems.Length && index < RewardCounts.Length
            Form rewardItem = RewardItems[index]
            Int rewardCount = RewardCounts[index]
            If ItemStages[index] == auiStageID && rewardItem != None && rewardCount > 0
                playerRef.AddItem(rewardItem, rewardCount, True)
            EndIf
            index += 1
        EndWhile
    EndIf
EndFunction

Bool Function HasRewardForStage(Int auiStageID)
    Int index = 0
    If XPStages != None
        While index < XPStages.Length
            If XPStages[index] == auiStageID
                Return True
            EndIf
            index += 1
        EndWhile
    EndIf

    index = 0
    If CapsStages != None
        While index < CapsStages.Length
            If CapsStages[index] == auiStageID
                Return True
            EndIf
            index += 1
        EndWhile
    EndIf

    index = 0
    If ItemStages != None
        While index < ItemStages.Length
            If ItemStages[index] == auiStageID
                Return True
            EndIf
            index += 1
        EndWhile
    EndIf
    Return False
EndFunction

Bool Function HasGrantedStage(Int auiStageID)
    If GrantedStages == None
        Return False
    EndIf

    Int index = 0
    While index < GrantedStages.Length
        If GrantedStages[index] == auiStageID
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Function RecordGrantedStage(Int auiStageID)
    If GrantedStages == None
        GrantedStages = New Int[1]
        GrantedStages[0] = auiStageID
    Else
        GrantedStages.Add(auiStageID)
    EndIf
EndFunction
